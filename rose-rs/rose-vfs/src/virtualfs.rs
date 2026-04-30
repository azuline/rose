//! Virtual filesystem path parser, name generation, sanitizer, and visibility filter.
//!
//! This module implements:
//! - `VirtualPath`: semantic representation of a path in the VFS, with a parser
//! - `VirtualNameGenerator`: generates virtual directory/file names and maintains
//!   inverse mappings (name→ID) with a 2-hour TTL for media player compatibility
//! - `Sanitizer`: sanitizes entity names (artists/genres/labels/descriptors) and
//!   maintains bidirectional mappings, using `DashMap` to avoid deadlocks
//! - `CanShower`: whitelist/blacklist filter for VFS entity visibility

use std::collections::{HashMap, HashSet};
use std::path::Path;

use dashmap::DashMap;
use tracing::debug;

use rose_core::common::{sanitize_dirname, sanitize_filename};
use rose_core::config::{Config, VirtualFSConfig};
use rose_core::templates::{
    evaluate_release_template, evaluate_track_template, PathContext, PathTemplate, Release, Track,
};

use crate::state::TTLCache;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Sentinel name for the "All Tracks" virtual directory.
pub const ALL_TRACKS: &str = "!All Tracks";

/// Blacklisted filenames that immediately return ENOENT.
const BLACKLISTED_NAMES: &[&str] = &[
    ".git",
    ".DS_Store",
    ".Trash",
    ".Trash-1000",
    "HEAD",
    ".envrc",
];

/// TTL for VirtualNameGenerator caches: 2 hours in seconds.
const VNAME_TTL_SECONDS: u64 = 60 * 60 * 2;

// ---------------------------------------------------------------------------
// ViewType
// ---------------------------------------------------------------------------

/// The 12 top-level views in the Rose virtual filesystem, plus Root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViewType {
    Root,
    Releases,
    Artists,
    Genres,
    Descriptors,
    Labels,
    LooseTracks,
    Collages,
    Playlists,
    New,
    Favorites,
    AddedOn,
    ReleasedOn,
}

// ---------------------------------------------------------------------------
// VirtualPath
// ---------------------------------------------------------------------------

/// A semantic representation of a path in the virtual filesystem.
///
/// Produced by `VirtualPath::parse()`. All fields except `view` are populated
/// only when the path descends deep enough into the view hierarchy.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VirtualPath {
    pub view: Option<ViewType>,
    pub artist: Option<String>,
    pub genre: Option<String>,
    pub descriptor: Option<String>,
    pub label: Option<String>,
    pub collage: Option<String>,
    pub playlist: Option<String>,
    /// May be set to `ALL_TRACKS` ("!All Tracks"), in which case it is not
    /// resolved to a release but treated as a special virtual directory.
    pub release: Option<String>,
    pub file: Option<String>,
}

impl VirtualPath {
    /// Construct a minimal VirtualPath with only the view set.
    fn with_view(view: ViewType) -> Self {
        Self {
            view: Some(view),
            artist: None,
            genre: None,
            descriptor: None,
            label: None,
            collage: None,
            playlist: None,
            release: None,
            file: None,
        }
    }

    /// Parse a filesystem path into a `VirtualPath`.
    ///
    /// Returns `Err(libc::ENOENT)` for blacklisted filenames, unrecognized
    /// top-level prefixes, or paths that exceed the maximum depth for their view.
    pub fn parse(path: &Path) -> Result<Self, i32> {
        let path_str = path.to_string_lossy();

        // Normalize: strip leading "/" and split on "/".
        let trimmed = path_str.trim_start_matches('/');
        if trimmed.is_empty() {
            return Ok(Self::with_view(ViewType::Root));
        }

        let parts: Vec<&str> = trimmed.split('/').collect();

        // Blacklist check on last segment.
        if let Some(&last) = parts.last() {
            if BLACKLISTED_NAMES.contains(&last) {
                debug!(
                    "Raising ENOENT early in VirtualPath parser: last segment '{}' is blacklisted",
                    last
                );
                return Err(libc::ENOENT);
            }
        }

        match parts[0] {
            "1. Releases" => parse_release_like(ViewType::Releases, &parts),
            "1. Releases - New" => parse_release_like(ViewType::New, &parts),
            "1. Releases - Favorites" => parse_release_like(ViewType::Favorites, &parts),
            "1. Releases - Added On" => parse_release_like(ViewType::AddedOn, &parts),
            "1. Releases - Released On" => parse_release_like(ViewType::ReleasedOn, &parts),
            "2. Artists" => parse_entity_view(ViewType::Artists, &parts, EntityKind::Artist),
            "3. Genres" => parse_entity_view(ViewType::Genres, &parts, EntityKind::Genre),
            "4. Descriptors" => {
                parse_entity_view(ViewType::Descriptors, &parts, EntityKind::Descriptor)
            }
            "5. Labels" => parse_entity_view(ViewType::Labels, &parts, EntityKind::Label),
            "6. Loose Tracks" => parse_release_like(ViewType::LooseTracks, &parts),
            "7. Collages" => parse_collage_view(&parts),
            "8. Playlists" => parse_playlist_view(&parts),
            _ => Err(libc::ENOENT),
        }
    }

    // -- Parent accessors for VirtualNameGenerator / Sanitizer lookups --

    /// Parent path for release name lookup.
    pub fn release_parent(&self) -> VirtualPath {
        VirtualPath {
            view: self.view,
            artist: self.artist.clone(),
            genre: self.genre.clone(),
            descriptor: self.descriptor.clone(),
            label: self.label.clone(),
            collage: self.collage.clone(),
            playlist: None,
            release: None,
            file: None,
        }
    }

    /// Parent path for track name lookup.
    pub fn track_parent(&self) -> VirtualPath {
        VirtualPath {
            view: self.view,
            artist: self.artist.clone(),
            genre: self.genre.clone(),
            descriptor: self.descriptor.clone(),
            label: self.label.clone(),
            collage: self.collage.clone(),
            playlist: self.playlist.clone(),
            release: self.release.clone(),
            file: None,
        }
    }

    /// Parent path for artist sanitizer lookup.
    pub fn artist_parent(&self) -> VirtualPath {
        VirtualPath {
            view: self.view,
            ..Self::with_view(self.view.unwrap_or(ViewType::Root))
        }
    }

    /// Parent path for genre sanitizer lookup.
    pub fn genre_parent(&self) -> VirtualPath {
        Self::with_view(self.view.unwrap_or(ViewType::Root))
    }

    /// Parent path for descriptor sanitizer lookup.
    pub fn descriptor_parent(&self) -> VirtualPath {
        Self::with_view(self.view.unwrap_or(ViewType::Root))
    }

    /// Parent path for label sanitizer lookup.
    pub fn label_parent(&self) -> VirtualPath {
        Self::with_view(self.view.unwrap_or(ViewType::Root))
    }

    /// Parent path for collage sanitizer lookup.
    pub fn collage_parent(&self) -> VirtualPath {
        Self::with_view(self.view.unwrap_or(ViewType::Root))
    }

    /// Parent path for playlist sanitizer lookup.
    pub fn playlist_parent(&self) -> VirtualPath {
        Self::with_view(self.view.unwrap_or(ViewType::Root))
    }
}

// ---------------------------------------------------------------------------
// Path parsing helpers
// ---------------------------------------------------------------------------

/// Which entity kind is at the second level for entity-based views (Artists, Genres, etc.).
enum EntityKind {
    Artist,
    Genre,
    Descriptor,
    Label,
}

/// Parse "1. Releases", "1. Releases - New", etc. and "6. Loose Tracks".
/// Depth: 1 = view, 2 = release, 3 = file.
fn parse_release_like(view: ViewType, parts: &[&str]) -> Result<VirtualPath, i32> {
    match parts.len() {
        1 => Ok(VirtualPath::with_view(view)),
        2 => Ok(VirtualPath {
            release: Some(parts[1].to_string()),
            ..VirtualPath::with_view(view)
        }),
        3 => Ok(VirtualPath {
            release: Some(parts[1].to_string()),
            file: Some(parts[2].to_string()),
            ..VirtualPath::with_view(view)
        }),
        _ => Err(libc::ENOENT),
    }
}

/// Parse "2. Artists", "3. Genres", "4. Descriptors", "5. Labels".
/// Depth: 1 = view, 2 = entity, 3 = release, 4 = file.
fn parse_entity_view(view: ViewType, parts: &[&str], kind: EntityKind) -> Result<VirtualPath, i32> {
    let mut vp = VirtualPath::with_view(view);
    match parts.len() {
        1 => Ok(vp),
        2 => {
            set_entity(&mut vp, &kind, parts[1]);
            Ok(vp)
        }
        3 => {
            set_entity(&mut vp, &kind, parts[1]);
            vp.release = Some(parts[2].to_string());
            Ok(vp)
        }
        4 => {
            set_entity(&mut vp, &kind, parts[1]);
            vp.release = Some(parts[2].to_string());
            vp.file = Some(parts[3].to_string());
            Ok(vp)
        }
        _ => Err(libc::ENOENT),
    }
}

fn set_entity(vp: &mut VirtualPath, kind: &EntityKind, name: &str) {
    match kind {
        EntityKind::Artist => vp.artist = Some(name.to_string()),
        EntityKind::Genre => vp.genre = Some(name.to_string()),
        EntityKind::Descriptor => vp.descriptor = Some(name.to_string()),
        EntityKind::Label => vp.label = Some(name.to_string()),
    }
}

/// Parse "7. Collages". Depth: 1 = view, 2 = collage, 3 = release, 4 = file.
fn parse_collage_view(parts: &[&str]) -> Result<VirtualPath, i32> {
    let mut vp = VirtualPath::with_view(ViewType::Collages);
    match parts.len() {
        1 => Ok(vp),
        2 => {
            vp.collage = Some(parts[1].to_string());
            Ok(vp)
        }
        3 => {
            vp.collage = Some(parts[1].to_string());
            vp.release = Some(parts[2].to_string());
            Ok(vp)
        }
        4 => {
            vp.collage = Some(parts[1].to_string());
            vp.release = Some(parts[2].to_string());
            vp.file = Some(parts[3].to_string());
            Ok(vp)
        }
        _ => Err(libc::ENOENT),
    }
}

/// Parse "8. Playlists". Depth: 1 = view, 2 = playlist, 3 = file (no release level).
fn parse_playlist_view(parts: &[&str]) -> Result<VirtualPath, i32> {
    let mut vp = VirtualPath::with_view(ViewType::Playlists);
    match parts.len() {
        1 => Ok(vp),
        2 => {
            vp.playlist = Some(parts[1].to_string());
            Ok(vp)
        }
        3 => {
            vp.playlist = Some(parts[1].to_string());
            vp.file = Some(parts[2].to_string());
            Ok(vp)
        }
        _ => Err(libc::ENOENT),
    }
}

// ---------------------------------------------------------------------------
// VirtualNameGenerator
// ---------------------------------------------------------------------------

/// Generates virtual directory/file names for releases and tracks, and
/// maintains inverse mappings (name→entity ID) with a 2-hour TTL.
///
/// The TTL allows Rose to serve accesses to old paths even after metadata
/// changes. Old paths don't show in readdir but still resolve for media
/// players that cached them.
pub struct VirtualNameGenerator {
    max_filename_bytes: usize,
    /// (release_parent, vname) → release_id
    release_store: TTLCache<(VirtualPath, String), String>,
    /// (track_parent, vname) → track_id
    track_store: TTLCache<(VirtualPath, String), String>,
    /// Cache for expensive template evaluations: (parent, template_text, metahash, position) → name
    release_template_eval_cache: HashMap<(VirtualPath, String, String, Option<String>), String>,
    track_template_eval_cache: HashMap<(VirtualPath, String, String, Option<String>), String>,
}

impl VirtualNameGenerator {
    pub fn new(max_filename_bytes: usize) -> Self {
        Self {
            max_filename_bytes,
            release_store: TTLCache::new(VNAME_TTL_SECONDS),
            track_store: TTLCache::new(VNAME_TTL_SECONDS),
            release_template_eval_cache: HashMap::new(),
            track_template_eval_cache: HashMap::new(),
        }
    }

    /// Generate virtual directory names for a list of releases under `release_parent`.
    ///
    /// Returns a `Vec<(Release, String)>` of releases paired with their virtual
    /// directory names. Stores the name→ID mapping in the TTL cache.
    pub fn list_release_paths(
        &mut self,
        release_parent: &VirtualPath,
        releases: &[Release],
        config: &Config,
        sanitizer: &Sanitizer,
    ) -> Vec<(Release, String)> {
        let mut seen: HashSet<String> = HashSet::new();
        seen.insert(ALL_TRACKS.to_string());
        let prefix_pad_size = releases.len().to_string().len();
        let mut result = Vec::with_capacity(releases.len());

        for (idx, release) in releases.iter().enumerate() {
            // Determine the proper template.
            let template = select_release_template(release_parent, config);

            // Generate a position if we're in a collage.
            let position = if release_parent.collage.is_some() {
                Some(format!("{:0>width$}", idx + 1, width = prefix_pad_size))
            } else {
                None
            };

            // Check template eval cache.
            let cache_key = (
                release_parent.clone(),
                template.text.clone(),
                release.id.clone(), // using id as metahash proxy
                position.clone(),
            );

            let vname = if let Some(cached) = self.release_template_eval_cache.get(&cache_key) {
                cached.clone()
            } else {
                let context = build_path_context(release_parent, sanitizer);
                let rendered = evaluate_release_template(
                    template,
                    release,
                    Some(&context),
                    position.as_deref(),
                );
                let sanitized = sanitize_dirname(self.max_filename_bytes, &rendered, false, false);
                self.release_template_eval_cache
                    .insert(cache_key, sanitized.clone());
                sanitized
            };

            // Handle name collisions.
            let mut final_name = vname.clone();
            let mut collision_no = 2u32;
            while seen.contains(&final_name) {
                final_name = format!("{} [{}]", vname, collision_no);
                collision_no += 1;
            }
            seen.insert(final_name.clone());

            // Store in TTL cache.
            self.release_store.insert(
                (release_parent.clone(), final_name.clone()),
                release.id.clone(),
            );

            result.push((release.clone(), final_name));
        }

        result
    }

    /// Generate virtual filenames for a list of tracks under `track_parent`.
    ///
    /// Returns a `Vec<(Track, String)>` of tracks paired with their virtual
    /// filenames. Stores the name→ID mapping in the TTL cache.
    pub fn list_track_paths(
        &mut self,
        track_parent: &VirtualPath,
        tracks: &[Track],
        config: &Config,
        sanitizer: &Sanitizer,
    ) -> Vec<(Track, String)> {
        let mut seen: HashSet<String> = HashSet::new();
        let prefix_pad_size = tracks.len().to_string().len();
        let mut result = Vec::with_capacity(tracks.len());

        for (idx, track) in tracks.iter().enumerate() {
            let template = select_track_template(track_parent, config);

            // Generate a position if we're in a playlist.
            let position = if track_parent.playlist.is_some() {
                Some(format!("{:0>width$}", idx + 1, width = prefix_pad_size))
            } else {
                None
            };

            let cache_key = (
                track_parent.clone(),
                template.text.clone(),
                track.id.clone(),
                position.clone(),
            );

            let vname = if let Some(cached) = self.track_template_eval_cache.get(&cache_key) {
                cached.clone()
            } else {
                let context = build_path_context(track_parent, sanitizer);
                let rendered =
                    evaluate_track_template(template, track, Some(&context), position.as_deref());
                let sanitized = sanitize_filename(self.max_filename_bytes, &rendered, false, false);
                self.track_template_eval_cache
                    .insert(cache_key, sanitized.clone());
                sanitized
            };

            // Handle collisions: insert [N] before file extension.
            let mut final_name = vname.clone();
            let mut collision_no = 2u32;
            while seen.contains(&final_name) {
                let p = Path::new(&vname);
                let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or(&vname);
                let ext = p.extension().and_then(|e| e.to_str());
                final_name = match ext {
                    Some(e) => format!("{} [{}].{}", stem, collision_no, e),
                    None => format!("{} [{}]", stem, collision_no),
                };
                collision_no += 1;
            }
            seen.insert(final_name.clone());

            // Store in TTL cache.
            self.track_store
                .insert((track_parent.clone(), final_name.clone()), track.id.clone());

            result.push((track.clone(), final_name));
        }

        result
    }

    /// Look up a release ID by virtual path. Returns `None` if not cached or expired.
    pub fn lookup_release(&mut self, p: &VirtualPath) -> Option<String> {
        let release_name = p.release.as_ref()?;
        let key = (p.release_parent(), release_name.clone());
        self.release_store.get(&key).cloned()
    }

    /// Look up a track ID by virtual path. Returns `None` if not cached or expired.
    pub fn lookup_track(&mut self, p: &VirtualPath) -> Option<String> {
        let file_name = p.file.as_ref()?;
        let key = (p.track_parent(), file_name.clone());
        self.track_store.get(&key).cloned()
    }
}

/// Select the release template for a given parent view.
fn select_release_template<'a>(parent: &VirtualPath, config: &'a Config) -> &'a PathTemplate {
    match parent.view {
        Some(ViewType::Releases) => &config.path_templates.releases.release,
        Some(ViewType::Favorites) => &config.path_templates.releases_favorite.release,
        Some(ViewType::New) => &config.path_templates.releases_new.release,
        Some(ViewType::AddedOn) => &config.path_templates.releases_added_on.release,
        Some(ViewType::ReleasedOn) => &config.path_templates.releases_released_on.release,
        Some(ViewType::Artists) => &config.path_templates.artists.release,
        Some(ViewType::Genres) => &config.path_templates.genres.release,
        Some(ViewType::Descriptors) => &config.path_templates.descriptors.release,
        Some(ViewType::Labels) => &config.path_templates.labels.release,
        Some(ViewType::LooseTracks) => &config.path_templates.loose_tracks.release,
        Some(ViewType::Collages) => &config.path_templates.collages.release,
        _ => &config.path_templates.releases.release, // fallback
    }
}

/// Select the track template for a given parent view.
fn select_track_template<'a>(parent: &VirtualPath, config: &'a Config) -> &'a PathTemplate {
    let is_all_tracks = parent.release.as_deref() == Some(ALL_TRACKS);

    if is_all_tracks {
        match parent.view {
            Some(ViewType::Releases) => &config.path_templates.releases.all_tracks,
            Some(ViewType::New) => &config.path_templates.releases_new.all_tracks,
            Some(ViewType::Favorites) => &config.path_templates.releases_favorite.all_tracks,
            Some(ViewType::AddedOn) => &config.path_templates.releases_added_on.all_tracks,
            Some(ViewType::ReleasedOn) => &config.path_templates.releases_released_on.all_tracks,
            Some(ViewType::Artists) => &config.path_templates.artists.all_tracks,
            Some(ViewType::Genres) => &config.path_templates.genres.all_tracks,
            Some(ViewType::Descriptors) => &config.path_templates.descriptors.all_tracks,
            Some(ViewType::Labels) => &config.path_templates.labels.all_tracks,
            Some(ViewType::LooseTracks) => &config.path_templates.loose_tracks.all_tracks,
            Some(ViewType::Collages) => &config.path_templates.collages.all_tracks,
            _ => &config.path_templates.releases.all_tracks,
        }
    } else {
        match parent.view {
            Some(ViewType::Releases) => &config.path_templates.releases.track,
            Some(ViewType::New) => &config.path_templates.releases_new.track,
            Some(ViewType::Favorites) => &config.path_templates.releases_favorite.track,
            Some(ViewType::AddedOn) => &config.path_templates.releases_added_on.track,
            Some(ViewType::ReleasedOn) => &config.path_templates.releases_released_on.track,
            Some(ViewType::Artists) => &config.path_templates.artists.track,
            Some(ViewType::Genres) => &config.path_templates.genres.track,
            Some(ViewType::Descriptors) => &config.path_templates.descriptors.track,
            Some(ViewType::Labels) => &config.path_templates.labels.track,
            Some(ViewType::LooseTracks) => &config.path_templates.loose_tracks.track,
            Some(ViewType::Collages) => &config.path_templates.collages.track,
            Some(ViewType::Playlists) => &config.path_templates.playlists,
            _ => &config.path_templates.releases.track,
        }
    }
}

/// Build a `PathContext` from the parent VirtualPath, unsanitizing entity names.
fn build_path_context(parent: &VirtualPath, sanitizer: &Sanitizer) -> PathContext {
    PathContext {
        genre: parent
            .genre
            .as_ref()
            .and_then(|g| sanitizer.unsanitize(g, &parent.genre_parent()).ok()),
        descriptor: parent
            .descriptor
            .as_ref()
            .and_then(|d| sanitizer.unsanitize(d, &parent.descriptor_parent()).ok()),
        label: parent
            .label
            .as_ref()
            .and_then(|l| sanitizer.unsanitize(l, &parent.label_parent()).ok()),
        artist: parent
            .artist
            .as_ref()
            .and_then(|a| sanitizer.unsanitize(a, &parent.artist_parent()).ok()),
        collage: parent.collage.clone(),
        playlist: parent.playlist.clone(),
    }
}

// ---------------------------------------------------------------------------
// Sanitizer
// ---------------------------------------------------------------------------

/// Sanitizes entity names (artists, genres, labels, descriptors) and maintains
/// bidirectional mappings.
///
/// Uses `DashMap` instead of `RwLock<HashMap>` because `unsanitize()` on a cache
/// miss triggers a `readdir()` which calls `sanitize()` — creating a circular
/// dependency that would deadlock with a regular lock.
pub struct Sanitizer {
    max_filename_bytes: usize,
    to_sanitized: DashMap<String, String>,
    to_unsanitized: DashMap<String, String>,
}

impl Sanitizer {
    pub fn new(max_filename_bytes: usize) -> Self {
        Self {
            max_filename_bytes,
            to_sanitized: DashMap::new(),
            to_unsanitized: DashMap::new(),
        }
    }

    /// Sanitize an entity name, caching the result.
    pub fn sanitize(&self, unsanitized: &str) -> String {
        if let Some(entry) = self.to_sanitized.get(unsanitized) {
            return entry.value().clone();
        }
        let sanitized = sanitize_dirname(self.max_filename_bytes, unsanitized, true, false);
        self.to_sanitized
            .insert(unsanitized.to_string(), sanitized.clone());
        self.to_unsanitized
            .insert(sanitized.clone(), unsanitized.to_string());
        sanitized
    }

    /// Reverse-lookup: get the original unsanitized name from a sanitized one.
    ///
    /// On cache miss, returns `Err(libc::ENOENT)`. In practice, the caller
    /// (RoseLogicalCore) will invoke `readdir()` on the parent before retrying,
    /// which populates the cache via `sanitize()`.
    pub fn unsanitize(&self, sanitized: &str, _parent: &VirtualPath) -> Result<String, i32> {
        if let Some(entry) = self.to_unsanitized.get(sanitized) {
            return Ok(entry.value().clone());
        }
        // The caller is responsible for triggering a readdir to populate the cache
        // and retrying. We return ENOENT here; the full circular-dependency pattern
        // (unsanitize → readdir → sanitize) is handled at the RoseLogicalCore level.
        debug!(
            "SANITIZER: Failed to find unsanitized string for '{}'; caller should readdir and retry",
            sanitized
        );
        Err(libc::ENOENT)
    }
}

// ---------------------------------------------------------------------------
// CanShower
// ---------------------------------------------------------------------------

/// Determines whether an artist, genre, descriptor, or label should be visible
/// in the virtual filesystem, based on configured whitelists and blacklists.
pub struct CanShower {
    artist_w: Option<HashSet<String>>,
    artist_b: Option<HashSet<String>>,
    genre_w: Option<HashSet<String>>,
    genre_b: Option<HashSet<String>>,
    descriptor_w: Option<HashSet<String>>,
    descriptor_b: Option<HashSet<String>>,
    label_w: Option<HashSet<String>>,
    label_b: Option<HashSet<String>>,
}

impl CanShower {
    pub fn new(vfs: &VirtualFSConfig) -> Self {
        Self {
            artist_w: vfs
                .artists_whitelist
                .as_ref()
                .map(|v| v.iter().cloned().collect()),
            artist_b: vfs
                .artists_blacklist
                .as_ref()
                .map(|v| v.iter().cloned().collect()),
            genre_w: vfs
                .genres_whitelist
                .as_ref()
                .map(|v| v.iter().cloned().collect()),
            genre_b: vfs
                .genres_blacklist
                .as_ref()
                .map(|v| v.iter().cloned().collect()),
            descriptor_w: vfs
                .descriptors_whitelist
                .as_ref()
                .map(|v| v.iter().cloned().collect()),
            descriptor_b: vfs
                .descriptors_blacklist
                .as_ref()
                .map(|v| v.iter().cloned().collect()),
            label_w: vfs
                .labels_whitelist
                .as_ref()
                .map(|v| v.iter().cloned().collect()),
            label_b: vfs
                .labels_blacklist
                .as_ref()
                .map(|v| v.iter().cloned().collect()),
        }
    }

    pub fn artist(&self, name: &str) -> bool {
        if let Some(w) = &self.artist_w {
            return w.contains(name);
        }
        if let Some(b) = &self.artist_b {
            return !b.contains(name);
        }
        true
    }

    pub fn genre(&self, name: &str) -> bool {
        if let Some(w) = &self.genre_w {
            return w.contains(name);
        }
        if let Some(b) = &self.genre_b {
            return !b.contains(name);
        }
        true
    }

    pub fn descriptor(&self, name: &str) -> bool {
        if let Some(w) = &self.descriptor_w {
            return w.contains(name);
        }
        if let Some(b) = &self.descriptor_b {
            return !b.contains(name);
        }
        true
    }

    pub fn label(&self, name: &str) -> bool {
        if let Some(w) = &self.label_w {
            return w.contains(name);
        }
        if let Some(b) = &self.label_b {
            return !b.contains(name);
        }
        true
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use rose_core::common::ArtistMapping;
    use rose_core::templates::{PathTemplateConfig, Release, Track};

    // -----------------------------------------------------------------------
    // VirtualPath::parse — all 12 view types at every valid depth
    // -----------------------------------------------------------------------

    #[test]
    fn parse_root() {
        let vp = VirtualPath::parse(Path::new("/")).unwrap();
        assert_eq!(vp.view, Some(ViewType::Root));
        assert!(vp.release.is_none());
        assert!(vp.file.is_none());
    }

    // -- Releases view --

    #[test]
    fn parse_releases_view() {
        let vp = VirtualPath::parse(Path::new("/1. Releases")).unwrap();
        assert_eq!(vp.view, Some(ViewType::Releases));
        assert!(vp.release.is_none());
    }

    #[test]
    fn parse_releases_release() {
        let vp = VirtualPath::parse(Path::new("/1. Releases/SomeAlbum")).unwrap();
        assert_eq!(vp.view, Some(ViewType::Releases));
        assert_eq!(vp.release.as_deref(), Some("SomeAlbum"));
        assert!(vp.file.is_none());
    }

    #[test]
    fn parse_releases_file() {
        let vp = VirtualPath::parse(Path::new("/1. Releases/SomeAlbum/track.flac")).unwrap();
        assert_eq!(vp.view, Some(ViewType::Releases));
        assert_eq!(vp.release.as_deref(), Some("SomeAlbum"));
        assert_eq!(vp.file.as_deref(), Some("track.flac"));
    }

    #[test]
    fn parse_releases_excess_depth() {
        assert_eq!(
            VirtualPath::parse(Path::new("/1. Releases/A/B/C")),
            Err(libc::ENOENT)
        );
    }

    // -- New view --

    #[test]
    fn parse_new_view() {
        let vp = VirtualPath::parse(Path::new("/1. Releases - New")).unwrap();
        assert_eq!(vp.view, Some(ViewType::New));
    }

    #[test]
    fn parse_new_release() {
        let vp = VirtualPath::parse(Path::new("/1. Releases - New/Album")).unwrap();
        assert_eq!(vp.view, Some(ViewType::New));
        assert_eq!(vp.release.as_deref(), Some("Album"));
    }

    #[test]
    fn parse_new_file() {
        let vp = VirtualPath::parse(Path::new("/1. Releases - New/Album/f.mp3")).unwrap();
        assert_eq!(vp.view, Some(ViewType::New));
        assert_eq!(vp.release.as_deref(), Some("Album"));
        assert_eq!(vp.file.as_deref(), Some("f.mp3"));
    }

    // -- Favorites view --

    #[test]
    fn parse_favorites_view() {
        let vp = VirtualPath::parse(Path::new("/1. Releases - Favorites")).unwrap();
        assert_eq!(vp.view, Some(ViewType::Favorites));
    }

    #[test]
    fn parse_favorites_file() {
        let vp =
            VirtualPath::parse(Path::new("/1. Releases - Favorites/Album/track.opus")).unwrap();
        assert_eq!(vp.view, Some(ViewType::Favorites));
        assert_eq!(vp.release.as_deref(), Some("Album"));
        assert_eq!(vp.file.as_deref(), Some("track.opus"));
    }

    // -- Added On view --

    #[test]
    fn parse_added_on_view() {
        let vp = VirtualPath::parse(Path::new("/1. Releases - Added On")).unwrap();
        assert_eq!(vp.view, Some(ViewType::AddedOn));
    }

    #[test]
    fn parse_added_on_release() {
        let vp = VirtualPath::parse(Path::new("/1. Releases - Added On/[2023] Album")).unwrap();
        assert_eq!(vp.view, Some(ViewType::AddedOn));
        assert_eq!(vp.release.as_deref(), Some("[2023] Album"));
    }

    // -- Released On view --

    #[test]
    fn parse_released_on_view() {
        let vp = VirtualPath::parse(Path::new("/1. Releases - Released On")).unwrap();
        assert_eq!(vp.view, Some(ViewType::ReleasedOn));
    }

    #[test]
    fn parse_released_on_file() {
        let vp = VirtualPath::parse(Path::new(
            "/1. Releases - Released On/[2016] Album/track.mp3",
        ))
        .unwrap();
        assert_eq!(vp.view, Some(ViewType::ReleasedOn));
        assert_eq!(vp.release.as_deref(), Some("[2016] Album"));
        assert_eq!(vp.file.as_deref(), Some("track.mp3"));
    }

    // -- Artists view --

    #[test]
    fn parse_artists_view() {
        let vp = VirtualPath::parse(Path::new("/2. Artists")).unwrap();
        assert_eq!(vp.view, Some(ViewType::Artists));
    }

    #[test]
    fn parse_artists_artist() {
        let vp = VirtualPath::parse(Path::new("/2. Artists/Kim Lip")).unwrap();
        assert_eq!(vp.view, Some(ViewType::Artists));
        assert_eq!(vp.artist.as_deref(), Some("Kim Lip"));
        assert!(vp.release.is_none());
    }

    #[test]
    fn parse_artists_release() {
        let vp = VirtualPath::parse(Path::new("/2. Artists/Kim Lip/Eclipse")).unwrap();
        assert_eq!(vp.artist.as_deref(), Some("Kim Lip"));
        assert_eq!(vp.release.as_deref(), Some("Eclipse"));
    }

    #[test]
    fn parse_artists_file() {
        let vp = VirtualPath::parse(Path::new("/2. Artists/Kim Lip/Eclipse/01.opus")).unwrap();
        assert_eq!(vp.artist.as_deref(), Some("Kim Lip"));
        assert_eq!(vp.release.as_deref(), Some("Eclipse"));
        assert_eq!(vp.file.as_deref(), Some("01.opus"));
    }

    #[test]
    fn parse_artists_excess_depth() {
        assert_eq!(
            VirtualPath::parse(Path::new("/2. Artists/A/B/C/D")),
            Err(libc::ENOENT)
        );
    }

    // -- Genres view --

    #[test]
    fn parse_genres_view() {
        let vp = VirtualPath::parse(Path::new("/3. Genres")).unwrap();
        assert_eq!(vp.view, Some(ViewType::Genres));
    }

    #[test]
    fn parse_genres_genre() {
        let vp = VirtualPath::parse(Path::new("/3. Genres/K-Pop")).unwrap();
        assert_eq!(vp.genre.as_deref(), Some("K-Pop"));
    }

    #[test]
    fn parse_genres_release() {
        let vp = VirtualPath::parse(Path::new("/3. Genres/K-Pop/Album")).unwrap();
        assert_eq!(vp.genre.as_deref(), Some("K-Pop"));
        assert_eq!(vp.release.as_deref(), Some("Album"));
    }

    #[test]
    fn parse_genres_file() {
        let vp = VirtualPath::parse(Path::new("/3. Genres/K-Pop/Album/track.mp3")).unwrap();
        assert_eq!(vp.genre.as_deref(), Some("K-Pop"));
        assert_eq!(vp.release.as_deref(), Some("Album"));
        assert_eq!(vp.file.as_deref(), Some("track.mp3"));
    }

    // -- Descriptors view --

    #[test]
    fn parse_descriptors_view() {
        let vp = VirtualPath::parse(Path::new("/4. Descriptors")).unwrap();
        assert_eq!(vp.view, Some(ViewType::Descriptors));
    }

    #[test]
    fn parse_descriptors_descriptor() {
        let vp = VirtualPath::parse(Path::new("/4. Descriptors/Mellow")).unwrap();
        assert_eq!(vp.descriptor.as_deref(), Some("Mellow"));
    }

    #[test]
    fn parse_descriptors_file() {
        let vp = VirtualPath::parse(Path::new("/4. Descriptors/Mellow/Album/track.mp3")).unwrap();
        assert_eq!(vp.descriptor.as_deref(), Some("Mellow"));
        assert_eq!(vp.release.as_deref(), Some("Album"));
        assert_eq!(vp.file.as_deref(), Some("track.mp3"));
    }

    // -- Labels view --

    #[test]
    fn parse_labels_view() {
        let vp = VirtualPath::parse(Path::new("/5. Labels")).unwrap();
        assert_eq!(vp.view, Some(ViewType::Labels));
    }

    #[test]
    fn parse_labels_label() {
        let vp = VirtualPath::parse(Path::new("/5. Labels/BIGHIT")).unwrap();
        assert_eq!(vp.label.as_deref(), Some("BIGHIT"));
    }

    #[test]
    fn parse_labels_file() {
        let vp = VirtualPath::parse(Path::new("/5. Labels/BIGHIT/Album/track.mp3")).unwrap();
        assert_eq!(vp.label.as_deref(), Some("BIGHIT"));
        assert_eq!(vp.release.as_deref(), Some("Album"));
        assert_eq!(vp.file.as_deref(), Some("track.mp3"));
    }

    // -- Loose Tracks view --

    #[test]
    fn parse_loose_tracks_view() {
        let vp = VirtualPath::parse(Path::new("/6. Loose Tracks")).unwrap();
        assert_eq!(vp.view, Some(ViewType::LooseTracks));
    }

    #[test]
    fn parse_loose_tracks_release() {
        let vp = VirtualPath::parse(Path::new("/6. Loose Tracks/SomeSingle")).unwrap();
        assert_eq!(vp.view, Some(ViewType::LooseTracks));
        assert_eq!(vp.release.as_deref(), Some("SomeSingle"));
    }

    #[test]
    fn parse_loose_tracks_file() {
        let vp = VirtualPath::parse(Path::new("/6. Loose Tracks/SomeSingle/track.mp3")).unwrap();
        assert_eq!(vp.release.as_deref(), Some("SomeSingle"));
        assert_eq!(vp.file.as_deref(), Some("track.mp3"));
    }

    // -- Collages view --

    #[test]
    fn parse_collages_view() {
        let vp = VirtualPath::parse(Path::new("/7. Collages")).unwrap();
        assert_eq!(vp.view, Some(ViewType::Collages));
    }

    #[test]
    fn parse_collages_collage() {
        let vp = VirtualPath::parse(Path::new("/7. Collages/MyCollage")).unwrap();
        assert_eq!(vp.collage.as_deref(), Some("MyCollage"));
    }

    #[test]
    fn parse_collages_release() {
        let vp = VirtualPath::parse(Path::new("/7. Collages/MyCollage/Album")).unwrap();
        assert_eq!(vp.collage.as_deref(), Some("MyCollage"));
        assert_eq!(vp.release.as_deref(), Some("Album"));
    }

    #[test]
    fn parse_collages_file() {
        let vp = VirtualPath::parse(Path::new("/7. Collages/MyCollage/Album/track.mp3")).unwrap();
        assert_eq!(vp.collage.as_deref(), Some("MyCollage"));
        assert_eq!(vp.release.as_deref(), Some("Album"));
        assert_eq!(vp.file.as_deref(), Some("track.mp3"));
    }

    #[test]
    fn parse_collages_excess_depth() {
        assert_eq!(
            VirtualPath::parse(Path::new("/7. Collages/C/R/F/X")),
            Err(libc::ENOENT)
        );
    }

    // -- Playlists view --

    #[test]
    fn parse_playlists_view() {
        let vp = VirtualPath::parse(Path::new("/8. Playlists")).unwrap();
        assert_eq!(vp.view, Some(ViewType::Playlists));
    }

    #[test]
    fn parse_playlists_playlist() {
        let vp = VirtualPath::parse(Path::new("/8. Playlists/Chill")).unwrap();
        assert_eq!(vp.playlist.as_deref(), Some("Chill"));
    }

    #[test]
    fn parse_playlists_file() {
        let vp = VirtualPath::parse(Path::new("/8. Playlists/Chill/track.mp3")).unwrap();
        assert_eq!(vp.playlist.as_deref(), Some("Chill"));
        assert_eq!(vp.file.as_deref(), Some("track.mp3"));
    }

    #[test]
    fn parse_playlists_excess_depth() {
        // Playlists have no release level: playlist/file only.
        assert_eq!(
            VirtualPath::parse(Path::new("/8. Playlists/P/A/B")),
            Err(libc::ENOENT)
        );
    }

    // -----------------------------------------------------------------------
    // Blacklisted filenames
    // -----------------------------------------------------------------------

    #[test]
    fn parse_blacklisted_git() {
        assert_eq!(
            VirtualPath::parse(Path::new("/1. Releases/.git")),
            Err(libc::ENOENT)
        );
    }

    #[test]
    fn parse_blacklisted_ds_store() {
        assert_eq!(
            VirtualPath::parse(Path::new("/.DS_Store")),
            Err(libc::ENOENT)
        );
    }

    #[test]
    fn parse_blacklisted_trash() {
        assert_eq!(VirtualPath::parse(Path::new("/.Trash")), Err(libc::ENOENT));
    }

    #[test]
    fn parse_blacklisted_trash_1000() {
        assert_eq!(
            VirtualPath::parse(Path::new("/.Trash-1000")),
            Err(libc::ENOENT)
        );
    }

    #[test]
    fn parse_blacklisted_head() {
        assert_eq!(VirtualPath::parse(Path::new("/HEAD")), Err(libc::ENOENT));
    }

    #[test]
    fn parse_blacklisted_envrc() {
        assert_eq!(VirtualPath::parse(Path::new("/.envrc")), Err(libc::ENOENT));
    }

    // -----------------------------------------------------------------------
    // Unrecognized prefix
    // -----------------------------------------------------------------------

    #[test]
    fn parse_unknown_prefix() {
        assert_eq!(
            VirtualPath::parse(Path::new("/9. Unknown")),
            Err(libc::ENOENT)
        );
    }

    // -----------------------------------------------------------------------
    // Parent helpers
    // -----------------------------------------------------------------------

    #[test]
    fn release_parent_strips_release_and_file() {
        let vp = VirtualPath::parse(Path::new("/2. Artists/Kim Lip/Eclipse/01.opus")).unwrap();
        let parent = vp.release_parent();
        assert_eq!(parent.view, Some(ViewType::Artists));
        assert_eq!(parent.artist.as_deref(), Some("Kim Lip"));
        assert!(parent.release.is_none());
        assert!(parent.file.is_none());
    }

    #[test]
    fn track_parent_strips_file_only() {
        let vp = VirtualPath::parse(Path::new("/2. Artists/Kim Lip/Eclipse/01.opus")).unwrap();
        let parent = vp.track_parent();
        assert_eq!(parent.view, Some(ViewType::Artists));
        assert_eq!(parent.artist.as_deref(), Some("Kim Lip"));
        assert_eq!(parent.release.as_deref(), Some("Eclipse"));
        assert!(parent.file.is_none());
    }

    #[test]
    fn collage_release_parent() {
        let vp = VirtualPath::parse(Path::new("/7. Collages/MyCollage/Album/track.mp3")).unwrap();
        let rp = vp.release_parent();
        assert_eq!(rp.collage.as_deref(), Some("MyCollage"));
        assert!(rp.release.is_none());
    }

    #[test]
    fn playlist_track_parent() {
        let vp = VirtualPath::parse(Path::new("/8. Playlists/Chill/track.mp3")).unwrap();
        let tp = vp.track_parent();
        assert_eq!(tp.playlist.as_deref(), Some("Chill"));
        assert!(tp.file.is_none());
    }

    // -----------------------------------------------------------------------
    // VirtualNameGenerator tests
    // -----------------------------------------------------------------------

    fn make_test_config() -> Config {
        Config {
            music_source_dir: PathBuf::from("/tmp/music"),
            cache_dir: PathBuf::from("/tmp/cache"),
            max_proc: 1,
            ignore_release_directories: vec![],
            rename_source_files: false,
            max_filename_bytes: 240,
            cover_art_stems: vec!["cover".into()],
            valid_art_exts: vec!["jpg".into()],
            write_parent_genres: false,
            artist_aliases_map: HashMap::new(),
            artist_aliases_parents_map: HashMap::new(),
            path_templates: PathTemplateConfig::with_defaults(),
            stored_metadata_rules: vec![],
            vfs: VirtualFSConfig {
                mount_dir: PathBuf::from("/tmp/vfs"),
                artists_whitelist: None,
                genres_whitelist: None,
                descriptors_whitelist: None,
                labels_whitelist: None,
                artists_blacklist: None,
                genres_blacklist: None,
                descriptors_blacklist: None,
                labels_blacklist: None,
                hide_genres_with_only_new_releases: false,
                hide_descriptors_with_only_new_releases: false,
                hide_labels_with_only_new_releases: false,
            },
        }
    }

    fn make_test_release(id: &str, title: &str) -> Release {
        Release {
            id: id.into(),
            source_path: PathBuf::from(format!("/tmp/music/{}", title)),
            added_at: "2023-01-01T00:00:00Z".into(),
            releasetitle: title.into(),
            releasetype: "album".into(),
            releasedate: None,
            originaldate: None,
            compositiondate: None,
            edition: None,
            catalognumber: None,
            new: false,
            favorite: false,
            rating: None,
            disctotal: 1,
            genres: vec![],
            parent_genres: vec![],
            secondary_genres: vec![],
            parent_secondary_genres: vec![],
            descriptors: vec![],
            labels: vec![],
            releaseartists: ArtistMapping {
                main: vec![rose_core::common::Artist::new("TestArtist")],
                ..Default::default()
            },
        }
    }

    fn make_test_track(id: &str, title: &str, release: &Release) -> Track {
        Track {
            id: id.into(),
            source_path: PathBuf::from(format!("{}/{}.opus", release.source_path.display(), title)),
            tracktitle: title.into(),
            tracknumber: "1".into(),
            tracktotal: 1,
            discnumber: "1".into(),
            duration_seconds: 200,
            trackartists: ArtistMapping {
                main: vec![rose_core::common::Artist::new("TestArtist")],
                ..Default::default()
            },
            release: release.clone(),
        }
    }

    #[test]
    fn vname_gen_list_release_paths_and_lookup() {
        let config = make_test_config();
        let sanitizer = Sanitizer::new(config.max_filename_bytes);
        let mut gen = VirtualNameGenerator::new(config.max_filename_bytes);

        let parent = VirtualPath::with_view(ViewType::Releases);
        let r1 = make_test_release("id-1", "Album One");
        let r2 = make_test_release("id-2", "Album Two");

        let results = gen.list_release_paths(&parent, &[r1, r2], &config, &sanitizer);
        assert_eq!(results.len(), 2);

        // Names should be non-empty.
        assert!(!results[0].1.is_empty());
        assert!(!results[1].1.is_empty());

        // Lookup should succeed.
        let lookup_path = VirtualPath {
            release: Some(results[0].1.clone()),
            ..VirtualPath::with_view(ViewType::Releases)
        };
        let found = gen.lookup_release(&lookup_path);
        assert_eq!(found, Some("id-1".to_string()));
    }

    #[test]
    fn vname_gen_release_collision_handling() {
        let config = make_test_config();
        let sanitizer = Sanitizer::new(config.max_filename_bytes);
        let mut gen = VirtualNameGenerator::new(config.max_filename_bytes);

        let parent = VirtualPath::with_view(ViewType::Releases);
        // Two releases with identical titles should get different names.
        let r1 = make_test_release("id-1", "Same Title");
        let r2 = make_test_release("id-2", "Same Title");

        let results = gen.list_release_paths(&parent, &[r1, r2], &config, &sanitizer);
        assert_ne!(results[0].1, results[1].1);
        // The second should have a collision suffix.
        assert!(results[1].1.contains("[2]"));
    }

    #[test]
    fn vname_gen_list_track_paths_and_lookup() {
        let config = make_test_config();
        let sanitizer = Sanitizer::new(config.max_filename_bytes);
        let mut gen = VirtualNameGenerator::new(config.max_filename_bytes);

        let release = make_test_release("rel-1", "Album");
        let parent = VirtualPath {
            release: Some("Album".into()),
            ..VirtualPath::with_view(ViewType::Releases)
        };

        let t1 = make_test_track("trk-1", "Track One", &release);
        let t2 = make_test_track("trk-2", "Track Two", &release);

        let results = gen.list_track_paths(&parent, &[t1, t2], &config, &sanitizer);
        assert_eq!(results.len(), 2);

        // Lookup should succeed.
        let lookup_path = VirtualPath {
            release: Some("Album".into()),
            file: Some(results[0].1.clone()),
            ..VirtualPath::with_view(ViewType::Releases)
        };
        let found = gen.lookup_track(&lookup_path);
        assert_eq!(found, Some("trk-1".to_string()));
    }

    #[test]
    fn vname_gen_track_collision_handling() {
        let config = make_test_config();
        let sanitizer = Sanitizer::new(config.max_filename_bytes);
        let mut gen = VirtualNameGenerator::new(config.max_filename_bytes);

        let release = make_test_release("rel-1", "Album");
        let parent = VirtualPath {
            release: Some("Album".into()),
            ..VirtualPath::with_view(ViewType::Releases)
        };

        // Same title tracks: collision handling adds [N] before extension.
        let t1 = make_test_track("trk-1", "Same", &release);
        let t2 = make_test_track("trk-2", "Same", &release);

        let results = gen.list_track_paths(&parent, &[t1, t2], &config, &sanitizer);
        assert_ne!(results[0].1, results[1].1);
        assert!(results[1].1.contains("[2]"));
    }

    #[test]
    fn vname_gen_lookup_before_list_returns_none() {
        let mut gen = VirtualNameGenerator::new(240);
        let p = VirtualPath {
            release: Some("Nonexistent".into()),
            ..VirtualPath::with_view(ViewType::Releases)
        };
        assert_eq!(gen.lookup_release(&p), None);
    }

    #[test]
    fn vname_gen_lookup_track_before_list_returns_none() {
        let mut gen = VirtualNameGenerator::new(240);
        let p = VirtualPath {
            release: Some("Album".into()),
            file: Some("track.mp3".into()),
            ..VirtualPath::with_view(ViewType::Releases)
        };
        assert_eq!(gen.lookup_track(&p), None);
    }

    // -----------------------------------------------------------------------
    // Sanitizer tests
    // -----------------------------------------------------------------------

    #[test]
    fn sanitizer_roundtrip() {
        let san = Sanitizer::new(240);
        let original = "Foo: Bar / Baz";
        let sanitized = san.sanitize(original);
        // Should have replaced illegal chars.
        assert!(!sanitized.contains(':'));
        assert!(!sanitized.contains('/'));
        // Unsanitize should return the original.
        let parent = VirtualPath::with_view(ViewType::Artists);
        let result = san.unsanitize(&sanitized, &parent);
        assert_eq!(result, Ok(original.to_string()));
    }

    #[test]
    fn sanitizer_cache_hit() {
        let san = Sanitizer::new(240);
        let s1 = san.sanitize("Hello World");
        let s2 = san.sanitize("Hello World");
        assert_eq!(s1, s2);
    }

    #[test]
    fn sanitizer_unsanitize_unknown_returns_enoent() {
        let san = Sanitizer::new(240);
        let parent = VirtualPath::with_view(ViewType::Artists);
        let result = san.unsanitize("totally_unknown", &parent);
        assert_eq!(result, Err(libc::ENOENT));
    }

    // -----------------------------------------------------------------------
    // CanShower tests
    // -----------------------------------------------------------------------

    fn make_vfs_config(wl: Option<Vec<String>>, bl: Option<Vec<String>>) -> VirtualFSConfig {
        VirtualFSConfig {
            mount_dir: PathBuf::from("/tmp"),
            artists_whitelist: wl.clone(),
            artists_blacklist: bl.clone(),
            genres_whitelist: wl.clone(),
            genres_blacklist: bl.clone(),
            descriptors_whitelist: wl.clone(),
            descriptors_blacklist: bl.clone(),
            labels_whitelist: wl,
            labels_blacklist: bl,
            hide_genres_with_only_new_releases: false,
            hide_descriptors_with_only_new_releases: false,
            hide_labels_with_only_new_releases: false,
        }
    }

    #[test]
    fn can_shower_no_filters() {
        let vfs = make_vfs_config(None, None);
        let cs = CanShower::new(&vfs);
        assert!(cs.artist("anything"));
        assert!(cs.genre("anything"));
        assert!(cs.descriptor("anything"));
        assert!(cs.label("anything"));
    }

    #[test]
    fn can_shower_whitelist_only() {
        let vfs = make_vfs_config(Some(vec!["allowed".into()]), None);
        let cs = CanShower::new(&vfs);
        assert!(cs.artist("allowed"));
        assert!(!cs.artist("blocked"));
        assert!(cs.genre("allowed"));
        assert!(!cs.genre("blocked"));
    }

    #[test]
    fn can_shower_blacklist_only() {
        let vfs = make_vfs_config(None, Some(vec!["blocked".into()]));
        let cs = CanShower::new(&vfs);
        assert!(cs.artist("allowed"));
        assert!(!cs.artist("blocked"));
        assert!(cs.genre("allowed"));
        assert!(!cs.genre("blocked"));
    }

    // -----------------------------------------------------------------------
    // ALL_TRACKS constant
    // -----------------------------------------------------------------------

    #[test]
    fn all_tracks_constant() {
        assert_eq!(ALL_TRACKS, "!All Tracks");
    }

    // -----------------------------------------------------------------------
    // VirtualPath with ALL_TRACKS
    // -----------------------------------------------------------------------

    #[test]
    fn parse_all_tracks_in_releases() {
        let vp = VirtualPath::parse(Path::new("/1. Releases/!All Tracks")).unwrap();
        assert_eq!(vp.view, Some(ViewType::Releases));
        assert_eq!(vp.release.as_deref(), Some(ALL_TRACKS));
    }

    #[test]
    fn parse_all_tracks_file() {
        let vp = VirtualPath::parse(Path::new("/1. Releases/!All Tracks/song.mp3")).unwrap();
        assert_eq!(vp.release.as_deref(), Some(ALL_TRACKS));
        assert_eq!(vp.file.as_deref(), Some("song.mp3"));
    }
}
