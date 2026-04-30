//! RoseLogicalCore — domain logic layer of the VFS.
//!
//! Translates virtual filesystem operations (getattr, readdir, unlink, mkdir,
//! rmdir, rename, open, read, write, release) into `rose-core` library calls.
//! This is the Rust port of `RoseLogicalCore` from `rose-vfs/rose_vfs/virtualfs.py`.

use std::collections::HashMap;
use std::ffi::CString;
use std::path::Path;

use tracing::debug;

use rose_core::audiotags::{AudioTags, SUPPORTED_AUDIO_EXTENSIONS};
use rose_core::cache::{
    self, get_collage, get_collage_releases, get_playlist, get_playlist_tracks, get_release,
    get_track, get_tracks_of_release, get_tracks_of_releases, list_artists, list_collages,
    list_descriptors, list_genres, list_labels, list_playlists, list_releases, list_tracks,
    release_within_collage, track_within_playlist, track_within_release, STORED_DATA_FILE_REGEX,
};
use rose_core::collages::{
    add_release_to_collage, create_collage, delete_collage, remove_release_from_collage,
    rename_collage,
};
use rose_core::config::Config;
use rose_core::playlists::{
    add_track_to_playlist, create_playlist, delete_playlist, delete_playlist_cover_art,
    remove_track_from_playlist, rename_playlist, set_playlist_cover_art,
};
use rose_core::releases::{delete_release, find_releases_matching_rule, set_release_cover_art};
use rose_core::rule_parser::{Matcher, Pattern};
use rose_core::tracks::find_tracks_matching_rule;

use crate::state::{CoverArtTarget, EntryAttrs, FileCreationSpecialOp, FileHandleManager};
use crate::virtualfs::{
    CanShower, Sanitizer, ViewType, VirtualNameGenerator, VirtualPath, ALL_TRACKS,
};

// ---------------------------------------------------------------------------
// libc helpers
// ---------------------------------------------------------------------------

/// Open a file via libc, returning the raw fd.
fn libc_open(path: &Path, flags: i32) -> Result<i32, i32> {
    let c_path = CString::new(path.to_string_lossy().as_bytes()).map_err(|_| libc::EINVAL)?;
    let fd = unsafe { libc::open(c_path.as_ptr(), flags, 0o644) };
    if fd < 0 {
        Err(libc::EIO)
    } else {
        Ok(fd)
    }
}

/// Read from fd at offset via pread.
fn libc_pread(fd: i32, buf: &mut [u8], offset: i64) -> Result<usize, i32> {
    let n = unsafe { libc::pread(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len(), offset) };
    if n < 0 {
        Err(libc::EIO)
    } else {
        Ok(n as usize)
    }
}

/// Write to fd at offset via pwrite.
fn libc_pwrite(fd: i32, data: &[u8], offset: i64) -> Result<usize, i32> {
    let n = unsafe { libc::pwrite(fd, data.as_ptr() as *const libc::c_void, data.len(), offset) };
    if n < 0 {
        Err(libc::EIO)
    } else {
        Ok(n as usize)
    }
}

/// Close a file descriptor.
fn libc_close(fd: i32) -> Result<(), i32> {
    let r = unsafe { libc::close(fd) };
    if r < 0 {
        Err(libc::EIO)
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// cache → templates type conversions
// ---------------------------------------------------------------------------

/// Convert a `cache::Release` to a `templates::Release` for template evaluation.
fn to_template_release(r: &cache::Release) -> rose_core::templates::Release {
    rose_core::templates::Release {
        id: r.id.clone(),
        source_path: r.source_path.clone(),
        added_at: r.added_at.clone(),
        releasetitle: r.releasetitle.clone(),
        releasetype: r.releasetype.clone(),
        releasedate: r.releasedate.clone(),
        originaldate: r.originaldate.clone(),
        compositiondate: r.compositiondate.clone(),
        edition: r.edition.clone(),
        catalognumber: r.catalognumber.clone(),
        new: r.new,
        favorite: r.favorite,
        rating: r.rating,
        disctotal: r.disctotal,
        genres: r.genres.clone(),
        parent_genres: r.parent_genres.clone(),
        secondary_genres: r.secondary_genres.clone(),
        parent_secondary_genres: r.parent_secondary_genres.clone(),
        descriptors: r.descriptors.clone(),
        labels: r.labels.clone(),
        releaseartists: r.releaseartists.clone(),
    }
}

/// Convert a `cache::Track` to a `templates::Track` for template evaluation.
fn to_template_track(t: &cache::Track) -> rose_core::templates::Track {
    rose_core::templates::Track {
        id: t.id.clone(),
        source_path: t.source_path.clone(),
        tracktitle: t.tracktitle.clone(),
        tracknumber: t.tracknumber.clone(),
        tracktotal: t.tracktotal,
        discnumber: t.discnumber.clone(),
        duration_seconds: t.duration_seconds,
        trackartists: t.trackartists.clone(),
        release: to_template_release(&t.release),
    }
}

/// Convert a slice of `cache::Release` to `templates::Release`.
fn to_template_releases(releases: &[cache::Release]) -> Vec<rose_core::templates::Release> {
    releases.iter().map(to_template_release).collect()
}

/// Convert a slice of `cache::Track` to `templates::Track`.
fn to_template_tracks(tracks: &[cache::Track]) -> Vec<rose_core::templates::Track> {
    tracks.iter().map(to_template_track).collect()
}

// ---------------------------------------------------------------------------
// RoseLogicalCore
// ---------------------------------------------------------------------------

/// The domain logic layer of the VFS. Receives parsed `VirtualPath` objects
/// and returns file attributes or performs mutations via `rose-core` calls.
pub struct RoseLogicalCore {
    pub config: Config,
    pub fhandler: FileHandleManager,
    pub sanitizer: Sanitizer,
    pub vnames: VirtualNameGenerator,
    pub can_show: CanShower,
    /// State for in-flight "file creation" operations across syscalls.
    pub file_creation_special_ops: HashMap<u64, FileCreationSpecialOp>,
    /// FH → release_id: trigger cache update on close for writable handles.
    pub update_release_on_fh_close: HashMap<u64, String>,
}

impl RoseLogicalCore {
    pub fn new(config: Config) -> Self {
        let max_filename_bytes = config.max_filename_bytes;
        let can_show = CanShower::new(&config.vfs);
        Self {
            config,
            fhandler: FileHandleManager::new(),
            sanitizer: Sanitizer::new(max_filename_bytes),
            vnames: VirtualNameGenerator::new(max_filename_bytes),
            can_show,
            file_creation_special_ops: HashMap::new(),
            update_release_on_fh_close: HashMap::new(),
        }
    }

    // -----------------------------------------------------------------------
    // getattr
    // -----------------------------------------------------------------------

    /// Look up attributes for a virtual path.
    pub fn getattr(&mut self, p: &VirtualPath) -> Result<EntryAttrs, i32> {
        debug!("LOGICAL: Received getattr for {:?}", p);

        // 8. Playlists
        if let Some(ref playlist_name) = p.playlist {
            let playlist = get_playlist(&self.config, playlist_name)
                .map_err(|_| libc::EIO)?
                .ok_or(libc::ENOENT)?;
            if let Some(ref file) = p.file {
                if let Some(ref cover_path) = playlist.cover_path {
                    let cover_name = format!(
                        "cover{}",
                        cover_path
                            .extension()
                            .map(|e| format!(".{}", e.to_string_lossy()))
                            .unwrap_or_default()
                    );
                    if *file == cover_name {
                        return Ok(EntryAttrs::stat("file", Some(cover_path)));
                    }
                }
                let track_id = self.get_track_id(p)?;
                if !track_within_playlist(&self.config, &track_id, playlist_name)
                    .map_err(|_| libc::EIO)?
                {
                    return Err(libc::ENOENT);
                }
                let track = get_track(&self.config, &track_id)
                    .map_err(|_| libc::EIO)?
                    .ok_or(libc::ENOENT)?;
                return Ok(EntryAttrs::stat("file", Some(&track.source_path)));
            }
            return Ok(EntryAttrs::stat("dir", None));
        }

        // 7. Collages
        if let Some(ref collage_name) = p.collage {
            if get_collage(&self.config, collage_name)
                .map_err(|_| libc::EIO)?
                .is_none()
            {
                return Err(libc::ENOENT);
            }
            if p.release.as_deref() == Some(ALL_TRACKS) {
                if p.file.is_none() {
                    return Ok(EntryAttrs::stat("dir", None));
                }
                let track_id = self.get_track_id(p)?;
                let track = get_track(&self.config, &track_id)
                    .map_err(|_| libc::EIO)?
                    .ok_or(libc::ENOENT)?;
                if !release_within_collage(&self.config, &track.release.id, collage_name)
                    .map_err(|_| libc::EIO)?
                {
                    return Err(libc::ENOENT);
                }
                return Ok(EntryAttrs::stat("file", Some(&track.source_path)));
            }
            if p.release.is_some() {
                return self.getattr_release(p);
            }
            return Ok(EntryAttrs::stat("dir", None));
        }

        // 5. Labels
        if let Some(ref label_san) = p.label {
            let la = self
                .sanitizer
                .unsanitize(label_san, &p.label_parent())
                .map_err(|_| libc::ENOENT)?;
            if !cache::label_exists(&self.config, &la).map_err(|_| libc::EIO)?
                || !self.can_show.label(&la)
            {
                return Err(libc::ENOENT);
            }
            if p.release.as_deref() == Some(ALL_TRACKS) {
                if p.file.is_none() {
                    return Ok(EntryAttrs::stat("dir", None));
                }
                let track_id = self.get_track_id(p)?;
                let track = get_track(&self.config, &track_id)
                    .map_err(|_| libc::EIO)?
                    .ok_or(libc::ENOENT)?;
                if !track.release.labels.contains(&la) {
                    return Err(libc::ENOENT);
                }
                return Ok(EntryAttrs::stat("file", Some(&track.source_path)));
            }
            if p.release.is_some() {
                return self.getattr_release(p);
            }
            return Ok(EntryAttrs::stat("dir", None));
        }

        // 4. Descriptors
        if let Some(ref desc_san) = p.descriptor {
            let d = self
                .sanitizer
                .unsanitize(desc_san, &p.descriptor_parent())
                .map_err(|_| libc::ENOENT)?;
            if !cache::descriptor_exists(&self.config, &d).map_err(|_| libc::EIO)?
                || !self.can_show.descriptor(&d)
            {
                return Err(libc::ENOENT);
            }
            if p.release.as_deref() == Some(ALL_TRACKS) {
                if p.file.is_none() {
                    return Ok(EntryAttrs::stat("dir", None));
                }
                let track_id = self.get_track_id(p)?;
                let track = get_track(&self.config, &track_id)
                    .map_err(|_| libc::EIO)?
                    .ok_or(libc::ENOENT)?;
                if !track.release.descriptors.contains(&d) {
                    return Err(libc::ENOENT);
                }
                return Ok(EntryAttrs::stat("file", Some(&track.source_path)));
            }
            if p.release.is_some() {
                return self.getattr_release(p);
            }
            return Ok(EntryAttrs::stat("dir", None));
        }

        // 3. Genres
        if let Some(ref genre_san) = p.genre {
            let g = self
                .sanitizer
                .unsanitize(genre_san, &p.genre_parent())
                .map_err(|_| libc::ENOENT)?;
            if !cache::genre_exists(&self.config, &g).map_err(|_| libc::EIO)?
                || !self.can_show.genre(&g)
            {
                return Err(libc::ENOENT);
            }
            if p.release.as_deref() == Some(ALL_TRACKS) {
                if p.file.is_none() {
                    return Ok(EntryAttrs::stat("dir", None));
                }
                let track_id = self.get_track_id(p)?;
                let track = get_track(&self.config, &track_id)
                    .map_err(|_| libc::EIO)?
                    .ok_or(libc::ENOENT)?;
                if !track.release.genres.contains(&g)
                    && !track.release.parent_genres.contains(&g)
                    && !track.release.secondary_genres.contains(&g)
                    && !track.release.parent_secondary_genres.contains(&g)
                {
                    return Err(libc::ENOENT);
                }
                return Ok(EntryAttrs::stat("file", Some(&track.source_path)));
            }
            if p.release.is_some() {
                return self.getattr_release(p);
            }
            return Ok(EntryAttrs::stat("dir", None));
        }

        // 2. Artists
        if let Some(ref artist_san) = p.artist {
            let a = self
                .sanitizer
                .unsanitize(artist_san, &p.artist_parent())
                .map_err(|_| libc::ENOENT)?;
            if !cache::artist_exists(&self.config, &a).map_err(|_| libc::EIO)?
                || !self.can_show.artist(&a)
            {
                return Err(libc::ENOENT);
            }
            if p.release.as_deref() == Some(ALL_TRACKS) {
                if p.file.is_none() {
                    return Ok(EntryAttrs::stat("dir", None));
                }
                let track_id = self.get_track_id(p)?;
                let track = get_track(&self.config, &track_id)
                    .map_err(|_| libc::EIO)?
                    .ok_or(libc::ENOENT)?;
                let artist_match = track
                    .release
                    .releaseartists
                    .all()
                    .iter()
                    .any(|art| art.name == a);
                if !artist_match {
                    return Err(libc::ENOENT);
                }
                return Ok(EntryAttrs::stat("file", Some(&track.source_path)));
            }
            if p.release.is_some() {
                return self.getattr_release(p);
            }
            return Ok(EntryAttrs::stat("dir", None));
        }

        // 1. Releases / New / Favorites / Added On / Released On / Loose Tracks
        if let Some(ref release_name) = p.release {
            if release_name == ALL_TRACKS {
                if p.file.is_none() {
                    return Ok(EntryAttrs::stat("dir", None));
                }
                let track_id = self.get_track_id(p)?;
                let track = get_track(&self.config, &track_id)
                    .map_err(|_| libc::EIO)?
                    .ok_or(libc::ENOENT)?;
                // For New/Favorites views, verify the release matches.
                if p.view == Some(ViewType::New) && !track.release.new {
                    return Err(libc::ENOENT);
                }
                if p.view == Some(ViewType::Favorites) && !track.release.favorite {
                    return Err(libc::ENOENT);
                }
                return Ok(EntryAttrs::stat("file", Some(&track.source_path)));
            }
            return self.getattr_release(p);
        }

        // 0. Root / View level
        if p.view.is_some() {
            return Ok(EntryAttrs::stat("dir", None));
        }

        Err(libc::ENOENT)
    }

    /// Common release getattr logic.
    fn getattr_release(&mut self, p: &VirtualPath) -> Result<EntryAttrs, i32> {
        let release_id = self.resolve_release_id(p)?;

        let release = get_release(&self.config, &release_id)
            .map_err(|_| libc::EIO)?
            .ok_or(libc::ENOENT)?;

        // No file → dir stat for release.
        if p.file.is_none() {
            return Ok(EntryAttrs::stat("dir", Some(&release.source_path)));
        }

        let file = p.file.as_ref().unwrap();

        // Cover art?
        if let Some(ref cover_path) = release.cover_image_path {
            let cover_name = format!(
                "cover{}",
                cover_path
                    .extension()
                    .map(|e| format!(".{}", e.to_string_lossy()))
                    .unwrap_or_default()
            );
            if *file == cover_name {
                return Ok(EntryAttrs::stat("file", Some(cover_path)));
            }
        }

        // .rose.{uuid}.toml?
        if *file == format!(".rose.{}.toml", release.id) {
            return Ok(EntryAttrs::stat("file", None));
        }

        // Track file?
        let track_id = self.get_track_id(p)?;
        if !track_within_release(&self.config, &track_id, &release.id).map_err(|_| libc::EIO)? {
            return Err(libc::ENOENT);
        }
        let track = get_track(&self.config, &track_id)
            .map_err(|_| libc::EIO)?
            .ok_or(libc::ENOENT)?;
        Ok(EntryAttrs::stat("file", Some(&track.source_path)))
    }

    // -----------------------------------------------------------------------
    // readdir
    // -----------------------------------------------------------------------

    /// List directory entries for a virtual path.
    pub fn readdir(&mut self, p: &VirtualPath) -> Result<Vec<(String, EntryAttrs)>, i32> {
        debug!("LOGICAL: Received readdir for {:?}", p);

        // Validate existence via getattr.
        self.getattr(p)?;

        let mut entries = vec![
            (".".to_string(), EntryAttrs::stat("dir", None)),
            ("..".to_string(), EntryAttrs::stat("dir", None)),
        ];

        // Root
        if p.view == Some(ViewType::Root) {
            for name in &[
                "1. Releases",
                "1. Releases - New",
                "1. Releases - Favorites",
                "1. Releases - Added On",
                "1. Releases - Released On",
                "2. Artists",
                "3. Genres",
                "4. Descriptors",
                "5. Labels",
                "6. Loose Tracks",
                "7. Collages",
                "8. Playlists",
            ] {
                entries.push((name.to_string(), EntryAttrs::stat("dir", None)));
            }
            return Ok(entries);
        }

        // ALL_TRACKS in artist/genre/descriptor/label/releases/new/favorites/loose tracks/added on/released on
        if p.release.as_deref() == Some(ALL_TRACKS)
            && (p.artist.is_some()
                || p.genre.is_some()
                || p.descriptor.is_some()
                || p.label.is_some()
                || matches!(
                    p.view,
                    Some(ViewType::Releases)
                        | Some(ViewType::ReleasedOn)
                        | Some(ViewType::AddedOn)
                        | Some(ViewType::New)
                        | Some(ViewType::Favorites)
                        | Some(ViewType::LooseTracks)
                ))
        {
            let matcher = self.build_entity_matcher(p);
            let tracks = if let Some(m) = matcher {
                find_tracks_matching_rule(&self.config, &m).map_err(|_| libc::EIO)?
            } else {
                list_tracks(&self.config, None).map_err(|_| libc::EIO)?
            };
            let track_parent = p.track_parent();
            let tmpl_tracks = to_template_tracks(&tracks);
            let track_paths = self.vnames.list_track_paths(
                &track_parent,
                &tmpl_tracks,
                &self.config,
                &self.sanitizer,
            );
            for (trk, vname) in track_paths {
                entries.push((vname, EntryAttrs::stat("file", Some(&trk.source_path))));
            }
            return Ok(entries);
        }

        // ALL_TRACKS in collage
        if p.release.as_deref() == Some(ALL_TRACKS) && p.collage.is_some() {
            let collage_name = p.collage.as_ref().unwrap();
            let releases =
                get_collage_releases(&self.config, collage_name).map_err(|_| libc::EIO)?;
            let release_tracks =
                get_tracks_of_releases(&self.config, &releases).map_err(|_| libc::EIO)?;
            let track_parent = p.track_parent();
            for (_release, tracks) in &release_tracks {
                let tmpl_tracks = to_template_tracks(tracks);
                let track_paths = self.vnames.list_track_paths(
                    &track_parent,
                    &tmpl_tracks,
                    &self.config,
                    &self.sanitizer,
                );
                for (trk, vname) in track_paths {
                    entries.push((vname, EntryAttrs::stat("file", Some(&trk.source_path))));
                }
            }
            return Ok(entries);
        }

        // Release directory
        if p.release.is_some() {
            let release_id = self.vnames.lookup_release(p).ok_or(libc::ENOENT)?;
            let release = get_release(&self.config, &release_id)
                .map_err(|_| libc::EIO)?
                .ok_or(libc::ENOENT)?;
            let tracks = get_tracks_of_release(&self.config, &release).map_err(|_| libc::EIO)?;
            let track_parent = p.track_parent();
            let tmpl_tracks = to_template_tracks(&tracks);
            let track_paths = self.vnames.list_track_paths(
                &track_parent,
                &tmpl_tracks,
                &self.config,
                &self.sanitizer,
            );
            for (trk, vname) in track_paths {
                entries.push((vname, EntryAttrs::stat("file", Some(&trk.source_path))));
            }
            if let Some(ref cover_path) = release.cover_image_path {
                let cover_name = format!(
                    "cover{}",
                    cover_path
                        .extension()
                        .map(|e| format!(".{}", e.to_string_lossy()))
                        .unwrap_or_default()
                );
                entries.push((cover_name, EntryAttrs::stat("file", Some(cover_path))));
            }
            entries.push((
                format!(".rose.{}.toml", release.id),
                EntryAttrs::stat("file", None),
            ));
            return Ok(entries);
        }

        // Artist/Genre/Descriptor/Label entity view → list releases
        if p.artist.is_some()
            || p.genre.is_some()
            || p.descriptor.is_some()
            || p.label.is_some()
            || matches!(
                p.view,
                Some(ViewType::Releases)
                    | Some(ViewType::New)
                    | Some(ViewType::Favorites)
                    | Some(ViewType::AddedOn)
                    | Some(ViewType::ReleasedOn)
            )
        {
            let matcher = self.build_release_entity_matcher(p);
            let releases = if let Some(m) = matcher {
                find_releases_matching_rule(&self.config, &m, false).map_err(|_| libc::EIO)?
            } else {
                list_releases(&self.config, None, false).map_err(|_| libc::EIO)?
            };

            entries.push((ALL_TRACKS.to_string(), EntryAttrs::stat("dir", None)));
            let release_parent = p.release_parent();
            let tmpl_releases = to_template_releases(&releases);
            let release_paths = self.vnames.list_release_paths(
                &release_parent,
                &tmpl_releases,
                &self.config,
                &self.sanitizer,
            );
            for (rls, vname) in release_paths {
                entries.push((vname, EntryAttrs::stat("dir", Some(&rls.source_path))));
            }
            return Ok(entries);
        }

        // Artists list
        if p.view == Some(ViewType::Artists) {
            let artists = list_artists(&self.config).map_err(|_| libc::EIO)?;
            for artist in artists {
                if !self.can_show.artist(&artist) {
                    continue;
                }
                entries.push((
                    self.sanitizer.sanitize(&artist),
                    EntryAttrs::stat("dir", None),
                ));
            }
            return Ok(entries);
        }

        // Genres list
        if p.view == Some(ViewType::Genres) {
            let genre_entries = list_genres(&self.config).map_err(|_| libc::EIO)?;
            for e in genre_entries {
                if !self.can_show.genre(&e.genre) {
                    continue;
                }
                if self.config.vfs.hide_genres_with_only_new_releases && e.only_new_releases {
                    continue;
                }
                entries.push((
                    self.sanitizer.sanitize(&e.genre),
                    EntryAttrs::stat("dir", None),
                ));
            }
            return Ok(entries);
        }

        // Descriptors list
        if p.view == Some(ViewType::Descriptors) {
            let desc_entries = list_descriptors(&self.config).map_err(|_| libc::EIO)?;
            for e in desc_entries {
                if !self.can_show.descriptor(&e.descriptor) {
                    continue;
                }
                if self.config.vfs.hide_descriptors_with_only_new_releases && e.only_new_releases {
                    continue;
                }
                entries.push((
                    self.sanitizer.sanitize(&e.descriptor),
                    EntryAttrs::stat("dir", None),
                ));
            }
            return Ok(entries);
        }

        // Labels list
        if p.view == Some(ViewType::Labels) {
            let label_entries = list_labels(&self.config).map_err(|_| libc::EIO)?;
            for e in label_entries {
                if !self.can_show.label(&e.label) {
                    continue;
                }
                if self.config.vfs.hide_labels_with_only_new_releases && e.only_new_releases {
                    continue;
                }
                entries.push((
                    self.sanitizer.sanitize(&e.label),
                    EntryAttrs::stat("dir", None),
                ));
            }
            return Ok(entries);
        }

        // Loose Tracks
        if p.view == Some(ViewType::LooseTracks) {
            let matcher = Matcher::from_expandable(
                &["releasetype"],
                Pattern::new("loosetrack", true, false, false, false),
            );
            let releases =
                find_releases_matching_rule(&self.config, &matcher, true).map_err(|_| libc::EIO)?;
            entries.push((ALL_TRACKS.to_string(), EntryAttrs::stat("dir", None)));
            let release_parent = p.release_parent();
            let tmpl_releases = to_template_releases(&releases);
            let release_paths = self.vnames.list_release_paths(
                &release_parent,
                &tmpl_releases,
                &self.config,
                &self.sanitizer,
            );
            for (rls, vname) in release_paths {
                entries.push((vname, EntryAttrs::stat("dir", Some(&rls.source_path))));
            }
            return Ok(entries);
        }

        // Collage with name
        if p.view == Some(ViewType::Collages) && p.collage.is_some() {
            let collage_name = p.collage.as_ref().unwrap();
            let releases =
                get_collage_releases(&self.config, collage_name).map_err(|_| libc::EIO)?;
            let release_parent = p.release_parent();
            let tmpl_releases = to_template_releases(&releases);
            let release_paths = self.vnames.list_release_paths(
                &release_parent,
                &tmpl_releases,
                &self.config,
                &self.sanitizer,
            );
            for (rls, vname) in release_paths {
                entries.push((vname, EntryAttrs::stat("dir", Some(&rls.source_path))));
            }
            entries.push((ALL_TRACKS.to_string(), EntryAttrs::stat("dir", None)));
            return Ok(entries);
        }

        // Collages list
        if p.view == Some(ViewType::Collages) {
            let collages = list_collages(&self.config).map_err(|_| libc::EIO)?;
            for collage in collages {
                entries.push((collage, EntryAttrs::stat("dir", None)));
            }
            return Ok(entries);
        }

        // Playlist with name
        if p.view == Some(ViewType::Playlists) && p.playlist.is_some() {
            let playlist_name = p.playlist.as_ref().unwrap();
            let playlist = get_playlist(&self.config, playlist_name)
                .map_err(|_| libc::EIO)?
                .ok_or(libc::ENOENT)?;
            if let Some(ref cover_path) = playlist.cover_path {
                let cover_name = format!(
                    "cover{}",
                    cover_path
                        .extension()
                        .map(|e| format!(".{}", e.to_string_lossy()))
                        .unwrap_or_default()
                );
                entries.push((cover_name, EntryAttrs::stat("file", Some(cover_path))));
            }
            let tracks = get_playlist_tracks(&self.config, playlist_name).map_err(|_| libc::EIO)?;
            let track_parent = p.track_parent();
            let tmpl_tracks = to_template_tracks(&tracks);
            let track_paths = self.vnames.list_track_paths(
                &track_parent,
                &tmpl_tracks,
                &self.config,
                &self.sanitizer,
            );
            for (trk, vname) in track_paths {
                entries.push((vname, EntryAttrs::stat("file", Some(&trk.source_path))));
            }
            return Ok(entries);
        }

        // Playlists list
        if p.view == Some(ViewType::Playlists) {
            let playlists = list_playlists(&self.config).map_err(|_| libc::EIO)?;
            for pname in playlists {
                entries.push((pname, EntryAttrs::stat("dir", None)));
            }
            return Ok(entries);
        }

        Err(libc::ENOENT)
    }

    // -----------------------------------------------------------------------
    // Mutation operations
    // -----------------------------------------------------------------------

    /// Delete a file from the VFS.
    pub fn unlink(&mut self, p: &VirtualPath) -> Result<(), i32> {
        debug!("LOGICAL: Received unlink for {:?}", p);

        // 1. Delete cover art from a playlist.
        if p.view == Some(ViewType::Playlists) && p.playlist.is_some() && p.file.is_some() {
            let playlist_name = p.playlist.as_ref().unwrap();
            let file = p.file.as_ref().unwrap();
            let valid_covers = self.config.valid_cover_arts();
            if valid_covers.iter().any(|c| c.eq_ignore_ascii_case(file))
                && get_playlist(&self.config, playlist_name)
                    .map_err(|_| libc::EIO)?
                    .is_some()
            {
                delete_playlist_cover_art(&self.config, playlist_name).map_err(|_| libc::EIO)?;
                return Ok(());
            }
            // 2. Delete a track from a playlist.
            if get_playlist(&self.config, playlist_name)
                .map_err(|_| libc::EIO)?
                .is_some()
            {
                if let Some(track_id) = self.vnames.lookup_track(p) {
                    remove_track_from_playlist(&self.config, playlist_name, &track_id)
                        .map_err(|_| libc::EIO)?;
                    return Ok(());
                }
            }
        }

        // Otherwise, noop — returning error prevents rmdir from working with rm.
        Ok(())
    }

    /// Create a directory in the VFS.
    pub fn mkdir(&mut self, p: &VirtualPath) -> Result<(), i32> {
        debug!("LOGICAL: Received mkdir for {:?}", p);

        // 1. Create a new collage.
        if p.collage.is_some() && p.release.is_none() {
            create_collage(&self.config, p.collage.as_ref().unwrap()).map_err(|_| libc::EIO)?;
            return Ok(());
        }
        // 2. Create a new playlist.
        if p.playlist.is_some() && p.file.is_none() {
            create_playlist(&self.config, p.playlist.as_ref().unwrap()).map_err(|_| libc::EIO)?;
            return Ok(());
        }

        Err(libc::EACCES)
    }

    /// Remove a directory from the VFS.
    pub fn rmdir(&mut self, p: &VirtualPath) -> Result<(), i32> {
        debug!("LOGICAL: Received rmdir for {:?}", p);

        // 1. Delete a collage.
        if p.view == Some(ViewType::Collages) && p.collage.is_some() && p.release.is_none() {
            delete_collage(&self.config, p.collage.as_ref().unwrap()).map_err(|_| libc::EIO)?;
            return Ok(());
        }
        // 2. Remove a release from a collage.
        if p.view == Some(ViewType::Collages) && p.collage.is_some() && p.release.is_some() {
            if let Some(release_id) = self.vnames.lookup_release(p) {
                remove_release_from_collage(&self.config, p.collage.as_ref().unwrap(), &release_id)
                    .map_err(|_| libc::EIO)?;
                return Ok(());
            }
        }
        // 3. Delete a playlist.
        if p.view == Some(ViewType::Playlists) && p.playlist.is_some() && p.file.is_none() {
            delete_playlist(&self.config, p.playlist.as_ref().unwrap()).map_err(|_| libc::EIO)?;
            return Ok(());
        }
        // 4. Delete a release (non-collage view).
        if p.view != Some(ViewType::Collages) && p.release.is_some() {
            if let Some(release_id) = self.vnames.lookup_release(p) {
                delete_release(&self.config, &release_id).map_err(|_| libc::EIO)?;
                return Ok(());
            }
        }

        Err(libc::EACCES)
    }

    /// Rename a path in the VFS.
    pub fn rename(&mut self, old: &VirtualPath, new: &VirtualPath) -> Result<(), i32> {
        debug!("LOGICAL: Received rename for {:?} -> {:?}", old, new);

        // 1. Rename a collage.
        if old.view == Some(ViewType::Collages)
            && new.view == Some(ViewType::Collages)
            && old.collage.is_some()
            && new.collage.is_some()
            && old.collage != new.collage
            && old.release.is_none()
            && new.release.is_none()
        {
            rename_collage(
                &self.config,
                old.collage.as_ref().unwrap(),
                new.collage.as_ref().unwrap(),
            )
            .map_err(|_| libc::EIO)?;
            return Ok(());
        }

        // 2. Rename a playlist.
        if old.view == Some(ViewType::Playlists)
            && new.view == Some(ViewType::Playlists)
            && old.playlist.is_some()
            && new.playlist.is_some()
            && old.playlist != new.playlist
            && old.file.is_none()
            && new.file.is_none()
        {
            rename_playlist(
                &self.config,
                old.playlist.as_ref().unwrap(),
                new.playlist.as_ref().unwrap(),
            )
            .map_err(|_| libc::EIO)?;
            return Ok(());
        }

        Err(libc::EACCES)
    }

    // -----------------------------------------------------------------------
    // File operations
    // -----------------------------------------------------------------------

    /// Open a file in the VFS.
    pub fn open(&mut self, p: &VirtualPath, flags: i32) -> Result<u64, i32> {
        debug!("LOGICAL: Received open for {:?} flags={}", p, flags);

        let err = if flags & libc::O_CREAT == libc::O_CREAT {
            libc::EACCES
        } else {
            libc::ENOENT
        };

        // 1. Add release to collage (O_CREAT + .rose.{uuid}.toml).
        if p.collage.is_some()
            && p.release.is_some()
            && p.file.is_some()
            && flags & libc::O_CREAT == libc::O_CREAT
        {
            let file = p.file.as_ref().unwrap();
            if let Some(caps) = STORED_DATA_FILE_REGEX.captures(file) {
                let release_id = caps[1].to_string();
                debug!(
                    "LOGICAL: Add release {} to collage {}, reached goal of collage addition sequence",
                    release_id,
                    p.collage.as_ref().unwrap()
                );
                add_release_to_collage(&self.config, p.collage.as_ref().unwrap(), &release_id)
                    .map_err(|_| libc::EIO)?;
                return Ok(self.fhandler.dev_null);
            }
        }

        // 2. Open ALL_TRACKS files.
        if p.release.as_deref() == Some(ALL_TRACKS) && p.file.is_some() {
            if let Some(track_id) = self.vnames.lookup_track(p) {
                if let Some(track) = get_track(&self.config, &track_id).map_err(|_| libc::EIO)? {
                    let host_fh = libc_open(&track.source_path, flags)?;
                    let fh = self.fhandler.wrap_host(host_fh);
                    if flags & libc::O_WRONLY == libc::O_WRONLY
                        || flags & libc::O_RDWR == libc::O_RDWR
                    {
                        self.update_release_on_fh_close
                            .insert(fh, track.release.id.clone());
                    }
                    return Ok(fh);
                }
            }
            return Err(err);
        }

        // 3. Open release files.
        if p.release.is_some() && p.file.is_some() {
            if let Some(release_id) = self.vnames.lookup_release(p) {
                if let Some(release) =
                    get_release(&self.config, &release_id).map_err(|_| libc::EIO)?
                {
                    let file = p.file.as_ref().unwrap();

                    // Music file?
                    if let Some(track_id) = self.vnames.lookup_track(p) {
                        if let Some(track) =
                            get_track(&self.config, &track_id).map_err(|_| libc::EIO)?
                        {
                            let host_fh = libc_open(&track.source_path, flags)?;
                            let fh = self.fhandler.wrap_host(host_fh);
                            if flags & libc::O_WRONLY == libc::O_WRONLY
                                || flags & libc::O_RDWR == libc::O_RDWR
                            {
                                self.update_release_on_fh_close
                                    .insert(fh, release.id.clone());
                            }
                            return Ok(fh);
                        }
                    }

                    // Datafile?
                    if *file == format!(".rose.{}.toml", release.id) {
                        let host_fh = libc_open(&release.source_path.join(file), flags)?;
                        return Ok(self.fhandler.wrap_host(host_fh));
                    }

                    // Existing cover art?
                    if let Some(ref cover_path) = release.cover_image_path {
                        let cover_name = format!(
                            "cover{}",
                            cover_path
                                .extension()
                                .map(|e| format!(".{}", e.to_string_lossy()))
                                .unwrap_or_default()
                        );
                        if *file == cover_name {
                            let host_fh = libc_open(cover_path, flags)?;
                            return Ok(self.fhandler.wrap_host(host_fh));
                        }
                    }

                    // New cover art (O_CREAT)?
                    let valid_covers = self.config.valid_cover_arts();
                    if valid_covers.iter().any(|c| c.eq_ignore_ascii_case(file))
                        && flags & libc::O_CREAT == libc::O_CREAT
                    {
                        let fh = self.fhandler.next();
                        let ext = Path::new(file)
                            .extension()
                            .map(|e| format!(".{}", e.to_string_lossy()))
                            .unwrap_or_default();
                        self.file_creation_special_ops.insert(
                            fh,
                            FileCreationSpecialOp::NewCoverArt {
                                entity: CoverArtTarget::Release(release.id.clone()),
                                ext,
                                data: Vec::new(),
                            },
                        );
                        return Ok(fh);
                    }

                    return Err(err);
                }
            }
        }

        // 4. Playlist files.
        if p.playlist.is_some() && p.file.is_some() {
            let playlist_name = p.playlist.as_ref().unwrap();
            let playlist = get_playlist(&self.config, playlist_name)
                .map_err(|_| libc::EIO)?
                .ok_or(libc::ENOENT)?;
            let file = p.file.as_ref().unwrap();
            let pf = Path::new(file);

            // Add track to playlist (audio file + O_CREAT).
            if let Some(ext_os) = pf.extension() {
                let ext_str = format!(".{}", ext_os.to_string_lossy().to_lowercase());
                if SUPPORTED_AUDIO_EXTENSIONS.contains(&ext_str.as_str())
                    && flags & libc::O_CREAT == libc::O_CREAT
                {
                    let fh = self.fhandler.next();
                    self.file_creation_special_ops.insert(
                        fh,
                        FileCreationSpecialOp::AddTrackToPlaylist {
                            playlist: playlist_name.clone(),
                            ext: ext_str,
                            data: Vec::new(),
                        },
                    );
                    return Ok(fh);
                }
            }

            // New cover art for playlist (O_CREAT).
            let valid_covers = self.config.valid_cover_arts();
            if valid_covers.iter().any(|c| c.eq_ignore_ascii_case(file))
                && flags & libc::O_CREAT == libc::O_CREAT
            {
                let fh = self.fhandler.next();
                let ext = pf
                    .extension()
                    .map(|e| format!(".{}", e.to_string_lossy()))
                    .unwrap_or_default();
                self.file_creation_special_ops.insert(
                    fh,
                    FileCreationSpecialOp::NewCoverArt {
                        entity: CoverArtTarget::Playlist(playlist_name.clone()),
                        ext,
                        data: Vec::new(),
                    },
                );
                return Ok(fh);
            }

            // Regular track open.
            if let Some(track_id) = self.vnames.lookup_track(p) {
                if let Some(track) = get_track(&self.config, &track_id).map_err(|_| libc::EIO)? {
                    let host_fh = libc_open(&track.source_path, flags)?;
                    let fh = self.fhandler.wrap_host(host_fh);
                    if flags & libc::O_WRONLY == libc::O_WRONLY
                        || flags & libc::O_RDWR == libc::O_RDWR
                    {
                        self.update_release_on_fh_close
                            .insert(fh, track.release.id.clone());
                    }
                    return Ok(fh);
                }
            }

            // Existing playlist cover.
            if let Some(ref cover_path) = playlist.cover_path {
                let cover_name = format!(
                    "cover{}",
                    cover_path
                        .extension()
                        .map(|e| format!(".{}", e.to_string_lossy()))
                        .unwrap_or_default()
                );
                if *file == cover_name {
                    let host_fh = libc_open(cover_path, flags)?;
                    return Ok(self.fhandler.wrap_host(host_fh));
                }
            }

            return Err(err);
        }

        Err(err)
    }

    /// Read data from an open file handle.
    pub fn read(&self, fh: u64, offset: i64, length: u32) -> Result<Vec<u8>, i32> {
        debug!(
            "LOGICAL: Received read for fh={} offset={} length={}",
            fh, offset, length
        );

        // Check special ops first.
        if let Some(sop) = self.file_creation_special_ops.get(&fh) {
            let data = match sop {
                FileCreationSpecialOp::AddTrackToPlaylist { data, .. } => data,
                FileCreationSpecialOp::NewCoverArt { data, .. } => data,
            };
            let start = offset as usize;
            let end = std::cmp::min(start + length as usize, data.len());
            if start >= data.len() {
                return Ok(Vec::new());
            }
            return Ok(data[start..end].to_vec());
        }

        let host_fh = self.fhandler.unwrap_host(fh).map_err(|_| libc::EBADF)?;
        let mut buf = vec![0u8; length as usize];
        let n = libc_pread(host_fh, &mut buf, offset)?;
        buf.truncate(n);
        Ok(buf)
    }

    /// Write data to an open file handle.
    pub fn write(&mut self, fh: u64, offset: i64, data: &[u8]) -> Result<u32, i32> {
        debug!(
            "LOGICAL: Received write for fh={} offset={} len={}",
            fh,
            offset,
            data.len()
        );

        // Check special ops first.
        if let Some(sop) = self.file_creation_special_ops.get_mut(&fh) {
            let buf = match sop {
                FileCreationSpecialOp::AddTrackToPlaylist { data: d, .. } => d,
                FileCreationSpecialOp::NewCoverArt { data: d, .. } => d,
            };
            let start = offset as usize;
            buf.truncate(start);
            buf.extend_from_slice(data);
            return Ok(data.len() as u32);
        }

        let host_fh = self.fhandler.unwrap_host(fh).map_err(|_| libc::EBADF)?;
        let n = libc_pwrite(host_fh, data, offset)?;
        Ok(n as u32)
    }

    /// Release (close) a file handle.
    pub fn release(&mut self, fh: u64) -> Result<(), i32> {
        debug!("LOGICAL: Received release for fh={}", fh);

        if let Some(sop) = self.file_creation_special_ops.remove(&fh) {
            match sop {
                FileCreationSpecialOp::AddTrackToPlaylist {
                    playlist,
                    ext,
                    data,
                } => {
                    if data.is_empty() {
                        debug!("LOGICAL: Aborting add-track-to-playlist: no bytes written");
                        return Ok(());
                    }
                    // Write to temp file, parse audio tags for track_id.
                    let tmpdir = tempfile::tempdir().map_err(|_| libc::EIO)?;
                    let audiopath = tmpdir.path().join(format!("f{ext}"));
                    std::fs::write(&audiopath, &data).map_err(|_| libc::EIO)?;
                    let audiofile = AudioTags::from_file(&audiopath).map_err(|_| libc::EIO)?;
                    let track_id = match audiofile.id {
                        Some(id) => id,
                        None => {
                            debug!(
                                "LOGICAL: Failed to parse track_id from file in playlist addition"
                            );
                            return Ok(());
                        }
                    };
                    add_track_to_playlist(&self.config, &playlist, &track_id)
                        .map_err(|_| libc::EIO)?;
                    return Ok(());
                }
                FileCreationSpecialOp::NewCoverArt { entity, ext, data } => {
                    if data.is_empty() {
                        debug!("LOGICAL: Aborting new-cover-art: no bytes written");
                        return Ok(());
                    }
                    let tmpdir = tempfile::tempdir().map_err(|_| libc::EIO)?;
                    let imagepath = tmpdir.path().join(format!("f{ext}"));
                    std::fs::write(&imagepath, &data).map_err(|_| libc::EIO)?;
                    match entity {
                        CoverArtTarget::Release(release_id) => {
                            set_release_cover_art(&self.config, &release_id, &imagepath)
                                .map_err(|_| libc::EIO)?;
                        }
                        CoverArtTarget::Playlist(playlist_name) => {
                            set_playlist_cover_art(&self.config, &playlist_name, &imagepath)
                                .map_err(|_| libc::EIO)?;
                        }
                    }
                    return Ok(());
                }
            }
        }

        // Trigger cache update if needed.
        if let Some(release_id) = self.update_release_on_fh_close.remove(&fh) {
            debug!(
                "LOGICAL: Triggering cache update for release {} after release syscall",
                release_id
            );
            if let Ok(Some(release)) = get_release(&self.config, &release_id) {
                let _ = cache::update_cache_for_releases(
                    &self.config,
                    Some(vec![release.source_path]),
                    false,
                );
            }
        }

        // Close the host FH.
        let host_fh = self.fhandler.unwrap_host(fh).map_err(|_| libc::EBADF)?;
        libc_close(host_fh)?;
        self.fhandler.release(fh);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Look up a track ID by virtual path, triggering readdir on cache miss.
    fn get_track_id(&mut self, p: &VirtualPath) -> Result<String, i32> {
        if let Some(id) = self.vnames.lookup_track(p) {
            return Ok(id);
        }
        debug!("LOGICAL: Invoking readdir before retrying track virtual name resolution");
        self.readdir(&p.track_parent())?;
        self.vnames.lookup_track(p).ok_or(libc::ENOENT)
    }

    /// Resolve a release ID by virtual path, triggering readdir on cache miss.
    fn resolve_release_id(&mut self, p: &VirtualPath) -> Result<String, i32> {
        if let Some(id) = self.vnames.lookup_release(p) {
            return Ok(id);
        }
        debug!("LOGICAL: Invoking readdir before retrying release virtual name resolution");
        self.readdir(&p.release_parent())?;
        self.vnames.lookup_release(p).ok_or(libc::ENOENT)
    }

    /// Build a Matcher for entity-based ALL_TRACKS readdir (tracks view).
    fn build_entity_matcher(&self, p: &VirtualPath) -> Option<Matcher> {
        if let Some(ref artist) = p.artist {
            let unsanitized = self.sanitizer.unsanitize(artist, &p.artist_parent()).ok()?;
            Some(Matcher::from_expandable(
                &["artist"],
                Pattern::new(&unsanitized, true, false, false, false),
            ))
        } else if let Some(ref genre) = p.genre {
            let unsanitized = self.sanitizer.unsanitize(genre, &p.genre_parent()).ok()?;
            Some(Matcher::from_expandable(
                &["genre"],
                Pattern::new(&unsanitized, true, false, false, false),
            ))
        } else if let Some(ref descriptor) = p.descriptor {
            let unsanitized = self
                .sanitizer
                .unsanitize(descriptor, &p.descriptor_parent())
                .ok()?;
            Some(Matcher::from_expandable(
                &["descriptor"],
                Pattern::new(&unsanitized, true, false, false, false),
            ))
        } else if let Some(ref label) = p.label {
            let unsanitized = self.sanitizer.unsanitize(label, &p.label_parent()).ok()?;
            Some(Matcher::from_expandable(
                &["label"],
                Pattern::new(&unsanitized, true, false, false, false),
            ))
        } else if p.view == Some(ViewType::New) {
            Some(Matcher::from_expandable(
                &["new"],
                Pattern::new("true", true, false, false, false),
            ))
        } else if p.view == Some(ViewType::Favorites) {
            Some(Matcher::from_expandable(
                &["favorite"],
                Pattern::new("true", true, false, false, false),
            ))
        } else if p.view == Some(ViewType::LooseTracks) {
            Some(Matcher::from_expandable(
                &["releasetype"],
                Pattern::new("loosetrack", true, false, false, false),
            ))
        } else {
            None
        }
    }

    /// Build a Matcher for entity-based release listing readdir.
    fn build_release_entity_matcher(&self, p: &VirtualPath) -> Option<Matcher> {
        if let Some(ref artist) = p.artist {
            let unsanitized = self.sanitizer.unsanitize(artist, &p.artist_parent()).ok()?;
            Some(Matcher::from_expandable(
                &["releaseartist"],
                Pattern::new(&unsanitized, true, false, false, false),
            ))
        } else if let Some(ref genre) = p.genre {
            let unsanitized = self.sanitizer.unsanitize(genre, &p.genre_parent()).ok()?;
            Some(Matcher::from_expandable(
                &["genre"],
                Pattern::new(&unsanitized, true, false, false, false),
            ))
        } else if let Some(ref descriptor) = p.descriptor {
            let unsanitized = self
                .sanitizer
                .unsanitize(descriptor, &p.descriptor_parent())
                .ok()?;
            Some(Matcher::from_expandable(
                &["descriptor"],
                Pattern::new(&unsanitized, true, false, false, false),
            ))
        } else if let Some(ref label) = p.label {
            let unsanitized = self.sanitizer.unsanitize(label, &p.label_parent()).ok()?;
            Some(Matcher::from_expandable(
                &["label"],
                Pattern::new(&unsanitized, true, false, false, false),
            ))
        } else if p.view == Some(ViewType::Favorites) {
            Some(Matcher::from_expandable(
                &["favorite"],
                Pattern::new("true", true, false, false, false),
            ))
        } else if p.view == Some(ViewType::New) {
            Some(Matcher::from_expandable(
                &["new"],
                Pattern::new("true", true, false, false, false),
            ))
        } else {
            None
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::Path;
    use tempfile::TempDir;

    use rose_core::cache::{connect, maybe_invalidate_cache_database};
    use rose_core::config::Config;

    use crate::virtualfs::{ViewType, VirtualPath};

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    /// Create a seeded database matching the canonical test fixture.
    /// Returns (TempDir, Config) — TempDir must be kept alive for the DB.
    fn seeded_config() -> (TempDir, Config) {
        let dir = TempDir::new().unwrap();
        let music_dir = dir.path().join("music");
        let cache_dir = dir.path().join("cache");
        std::fs::create_dir_all(&music_dir).unwrap();
        std::fs::create_dir_all(&cache_dir).unwrap();

        let cfg_path = dir.path().join("config.toml");
        let mut f = std::fs::File::create(&cfg_path).unwrap();
        write!(
            f,
            r#"
            music_source_dir = "{}"
            cache_dir = "{}"
            vfs.mount_dir = "{}"
            "#,
            music_dir.display(),
            cache_dir.display(),
            dir.path().join("vfs").display(),
        )
        .unwrap();
        let config = Config::parse(Some(&cfg_path)).unwrap();
        maybe_invalidate_cache_database(&config).unwrap();

        let dirpaths = [
            music_dir.join("r1"),
            music_dir.join("r2"),
            music_dir.join("r3"),
            music_dir.join("r4"),
        ];
        let musicpaths = [
            music_dir.join("r1/01.m4a"),
            music_dir.join("r1/02.m4a"),
            music_dir.join("r2/01.m4a"),
            music_dir.join("r3/01.m4a"),
            music_dir.join("r4/01.m4a"),
        ];
        let imagepaths = [
            music_dir.join("r2/cover.jpg"),
            music_dir.join("!playlists/Lala Lisa.jpg"),
        ];

        // Create actual files so that EntryAttrs::stat can stat them.
        for dp in &dirpaths {
            std::fs::create_dir_all(dp).unwrap();
        }
        for mp in &musicpaths {
            if let Some(parent) = mp.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(mp, b"fake audio data for testing").unwrap();
        }
        let playlists_dir = music_dir.join("!playlists");
        std::fs::create_dir_all(&playlists_dir).unwrap();
        for ip in &imagepaths {
            if let Some(parent) = ip.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(ip, b"fake image data").unwrap();
        }

        let conn = connect(&config).unwrap();
        conn.execute_batch(&format!(
            r#"
INSERT INTO releases
       (id  , source_path    , cover_image_path , added_at                   , datafile_mtime, title      , releasetype , releasedate , originaldate, compositiondate, catalognumber, edition , disctotal, new  , favorite, metahash)
VALUES ('r1', '{}'           , null             , '0000-01-01T00:00:00+00:00', '999'         , 'Release 1', 'album'     , '2023'      , null        , null           , null         , null    , 1        , false, true    , '1')
     , ('r2', '{}'           , '{}'             , '0000-01-01T00:00:00+00:00', '999'         , 'Release 2', 'album'     , '2021'      , '2019'      , null           , 'DG-001'     , 'Deluxe', 1        , true , false   , '2')
     , ('r3', '{}'           , null             , '0000-01-01T00:00:00+00:00', '999'         , 'Release 3', 'album'     , '2021-04-20', null        , '1780'         , 'DG-002'     , null    , 1        , false, false   , '3')
     , ('r4', '{}'           , null             , '0000-01-01T00:00:00+00:00', '999'         , 'Release 4', 'loosetrack', '2021-04-20', null        , '1780'         , 'DG-002'     , null    , 1        , false, false   , '4');

INSERT INTO releases_genres
       (release_id, genre             , position)
VALUES ('r1'      , 'Techno'          , 1)
     , ('r1'      , 'Deep House'      , 2)
     , ('r2'      , 'Modern Classical', 1);

INSERT INTO releases_secondary_genres
       (release_id, genre             , position)
VALUES ('r1'      , 'Rominimal'       , 1)
     , ('r1'      , 'Ambient'         , 2)
     , ('r2'      , 'Orchestral Music', 1);

INSERT INTO releases_descriptors
       (release_id, descriptor, position)
VALUES ('r1'      , 'Warm'    , 1)
     , ('r1'      , 'Hot'     , 2)
     , ('r2'      , 'Wet'     , 1);

INSERT INTO releases_labels
       (release_id, label         , position)
VALUES ('r1'      , 'Silk Music'  , 1)
     , ('r2'      , 'Native State', 1);

INSERT INTO tracks
       (id  , source_path    , source_mtime, title    , release_id, tracknumber, tracktotal, discnumber, duration_seconds, metahash)
VALUES ('t1', '{}'           , '999'       , 'Track 1', 'r1'      , '01'       , 2         , '01'      , 120             , '1')
     , ('t2', '{}'           , '999'       , 'Track 2', 'r1'      , '02'       , 2         , '01'      , 240             , '2')
     , ('t3', '{}'           , '999'       , 'Track 1', 'r2'      , '01'       , 1         , '01'      , 120             , '3')
     , ('t4', '{}'           , '999'       , 'Track 1', 'r3'      , '01'       , 1         , '01'      , 120             , '4')
     , ('t5', '{}'           , '999'       , 'Track 1', 'r4'      , '01'       , 1         , '01'      , 120             , '5');

INSERT INTO releases_artists
       (release_id, artist           , role   , position)
VALUES ('r1'      , 'Techno Man'     , 'main' , 1)
     , ('r1'      , 'Bass Man'       , 'main' , 2)
     , ('r2'      , 'Violin Woman'   , 'main' , 1)
     , ('r2'      , 'Conductor Woman', 'guest', 2);

INSERT INTO tracks_artists
       (track_id, artist           , role   , position)
VALUES ('t1'    , 'Techno Man'     , 'main' , 1)
     , ('t1'    , 'Bass Man'       , 'main' , 2)
     , ('t2'    , 'Techno Man'     , 'main' , 1)
     , ('t2'    , 'Bass Man'       , 'main' , 2)
     , ('t3'    , 'Violin Woman'   , 'main' , 1)
     , ('t3'    , 'Conductor Woman', 'guest', 2);

INSERT INTO collages
       (name       , source_mtime)
VALUES ('Rose Gold', '999')
     , ('Ruby Red' , '999');

INSERT INTO collages_releases
       (collage_name, release_id, position, missing)
VALUES ('Rose Gold' , 'r1'      , 1       , false)
     , ('Rose Gold' , 'r2'      , 2       , false);

INSERT INTO playlists
       (name           , source_mtime, cover_path)
VALUES ('Lala Lisa'    , '999'       , '{}')
     , ('Turtle Rabbit', '999'       , null);

INSERT INTO playlists_tracks
       (playlist_name, track_id, position, missing)
VALUES ('Lala Lisa'  , 't1'    , 1       , false)
     , ('Lala Lisa'  , 't3'    , 2       , false);
            "#,
            dirpaths[0].display(),
            dirpaths[1].display(), imagepaths[0].display(),
            dirpaths[2].display(),
            dirpaths[3].display(),
            musicpaths[0].display(),
            musicpaths[1].display(),
            musicpaths[2].display(),
            musicpaths[3].display(),
            musicpaths[4].display(),
            imagepaths[1].display(),
        ))
        .expect("Failed to seed cache database");

        (dir, config)
    }

    /// Create a seeded config with custom VFS whitelist/blacklist.
    fn seeded_config_with_vfs_options(
        artists_whitelist: Option<Vec<String>>,
        artists_blacklist: Option<Vec<String>>,
    ) -> (TempDir, Config) {
        let dir = TempDir::new().unwrap();
        let music_dir = dir.path().join("music");
        let cache_dir = dir.path().join("cache");
        std::fs::create_dir_all(&music_dir).unwrap();
        std::fs::create_dir_all(&cache_dir).unwrap();

        // Build VFS config section.
        let mut vfs_extra = String::new();
        if let Some(ref wl) = artists_whitelist {
            let items: Vec<String> = wl.iter().map(|s| format!("\"{}\"", s)).collect();
            vfs_extra.push_str(&format!("vfs.artists_whitelist = [{}]\n", items.join(", ")));
        }
        if let Some(ref bl) = artists_blacklist {
            let items: Vec<String> = bl.iter().map(|s| format!("\"{}\"", s)).collect();
            vfs_extra.push_str(&format!("vfs.artists_blacklist = [{}]\n", items.join(", ")));
        }

        let cfg_path = dir.path().join("config.toml");
        let mut f = std::fs::File::create(&cfg_path).unwrap();
        write!(
            f,
            r#"
            music_source_dir = "{}"
            cache_dir = "{}"
            vfs.mount_dir = "{}"
            {}
            "#,
            music_dir.display(),
            cache_dir.display(),
            dir.path().join("vfs").display(),
            vfs_extra,
        )
        .unwrap();
        let config = Config::parse(Some(&cfg_path)).unwrap();
        maybe_invalidate_cache_database(&config).unwrap();

        let dirpaths = [
            music_dir.join("r1"),
            music_dir.join("r2"),
            music_dir.join("r3"),
        ];
        let musicpaths = [
            music_dir.join("r1/01.m4a"),
            music_dir.join("r1/02.m4a"),
            music_dir.join("r2/01.m4a"),
            music_dir.join("r3/01.m4a"),
        ];

        for dp in &dirpaths {
            std::fs::create_dir_all(dp).unwrap();
        }
        for mp in &musicpaths {
            if let Some(parent) = mp.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(mp, b"fake audio data for testing").unwrap();
        }

        let conn = connect(&config).unwrap();
        conn.execute_batch(&format!(
            r#"
INSERT INTO releases
       (id  , source_path    , cover_image_path , added_at                   , datafile_mtime, title      , releasetype , releasedate , originaldate, compositiondate, catalognumber, edition , disctotal, new  , favorite, metahash)
VALUES ('r1', '{}'           , null             , '0000-01-01T00:00:00+00:00', '999'         , 'Release 1', 'album'     , '2023'      , null        , null           , null         , null    , 1        , false, true    , '1')
     , ('r2', '{}'           , null             , '0000-01-01T00:00:00+00:00', '999'         , 'Release 2', 'album'     , '2021'      , '2019'      , null           , 'DG-001'     , 'Deluxe', 1        , true , false   , '2')
     , ('r3', '{}'           , null             , '0000-01-01T00:00:00+00:00', '999'         , 'Release 3', 'album'     , '2021-04-20', null        , '1780'         , 'DG-002'     , null    , 1        , false, false   , '3');

INSERT INTO releases_genres
       (release_id, genre             , position)
VALUES ('r1'      , 'Techno'          , 1);

INSERT INTO tracks
       (id  , source_path    , source_mtime, title    , release_id, tracknumber, tracktotal, discnumber, duration_seconds, metahash)
VALUES ('t1', '{}'           , '999'       , 'Track 1', 'r1'      , '01'       , 2         , '01'      , 120             , '1')
     , ('t2', '{}'           , '999'       , 'Track 2', 'r1'      , '02'       , 2         , '01'      , 240             , '2')
     , ('t3', '{}'           , '999'       , 'Track 1', 'r2'      , '01'       , 1         , '01'      , 120             , '3')
     , ('t4', '{}'           , '999'       , 'Track 1', 'r3'      , '01'       , 1         , '01'      , 120             , '4');

INSERT INTO releases_artists
       (release_id, artist           , role   , position)
VALUES ('r1'      , 'Techno Man'     , 'main' , 1)
     , ('r1'      , 'Bass Man'       , 'main' , 2)
     , ('r2'      , 'Violin Woman'   , 'main' , 1)
     , ('r3'      , 'Conductor Woman', 'main' , 1);

INSERT INTO tracks_artists
       (track_id, artist           , role   , position)
VALUES ('t1'    , 'Techno Man'     , 'main' , 1)
     , ('t1'    , 'Bass Man'       , 'main' , 2)
     , ('t2'    , 'Techno Man'     , 'main' , 1)
     , ('t3'    , 'Violin Woman'   , 'main' , 1)
     , ('t4'    , 'Conductor Woman', 'main' , 1);
            "#,
            dirpaths[0].display(),
            dirpaths[1].display(),
            dirpaths[2].display(),
            musicpaths[0].display(),
            musicpaths[1].display(),
            musicpaths[2].display(),
            musicpaths[3].display(),
        ))
        .expect("Failed to seed cache database");

        (dir, config)
    }

    /// Helper: extract only the "real" entry names (excluding "." and "..").
    fn entry_names(entries: &[(String, EntryAttrs)]) -> Vec<String> {
        entries
            .iter()
            .filter(|(n, _)| n != "." && n != "..")
            .map(|(n, _)| n.clone())
            .collect()
    }

    // -----------------------------------------------------------------------
    // readdir tests
    // -----------------------------------------------------------------------

    // 1. readdir on root returns all 12 top-level view directories.
    #[test]
    fn test_readdir_root() {
        let (_dir, config) = seeded_config();
        let mut core = RoseLogicalCore::new(config);

        let root = VirtualPath::parse(Path::new("/")).unwrap();
        let entries = core.readdir(&root).unwrap();
        let names = entry_names(&entries);

        let expected = vec![
            "1. Releases",
            "1. Releases - New",
            "1. Releases - Favorites",
            "1. Releases - Added On",
            "1. Releases - Released On",
            "2. Artists",
            "3. Genres",
            "4. Descriptors",
            "5. Labels",
            "6. Loose Tracks",
            "7. Collages",
            "8. Playlists",
        ];
        assert_eq!(names.len(), expected.len(), "root should have 12 view dirs");
        for e in &expected {
            assert!(
                names.contains(&e.to_string()),
                "root should contain '{}', got {:?}",
                e,
                names
            );
        }
    }

    // 2. readdir on /Releases returns release virtual directory names.
    #[test]
    fn test_readdir_releases_view() {
        let (_dir, config) = seeded_config();
        let mut core = RoseLogicalCore::new(config);

        let vp = VirtualPath::parse(Path::new("/1. Releases")).unwrap();
        let entries = core.readdir(&vp).unwrap();
        let names = entry_names(&entries);

        // Should have 3 releases (r1, r2, r3 — not r4 which is loosetrack) + "!All Tracks"
        assert!(
            names.contains(&"!All Tracks".to_string()),
            "should contain !All Tracks"
        );
        // Releases view uses list_releases with include_loose_tracks=false for filter,
        // but actually goes through find_releases_matching_rule or list_releases.
        // The seeded data has r1, r2, r3 as albums and r4 as loosetrack.
        // The code calls list_releases(config, None, false) which excludes loose tracks.
        let release_names: Vec<&String> = names.iter().filter(|n| *n != "!All Tracks").collect();
        assert_eq!(
            release_names.len(),
            3,
            "should have 3 album releases, got {:?}",
            release_names
        );
    }

    // 3. readdir on /Artists returns artist names.
    #[test]
    fn test_readdir_artists_view() {
        let (_dir, config) = seeded_config();
        let mut core = RoseLogicalCore::new(config);

        let vp = VirtualPath::parse(Path::new("/2. Artists")).unwrap();
        let entries = core.readdir(&vp).unwrap();
        let names = entry_names(&entries);

        // Artists: Techno Man, Bass Man, Violin Woman, Conductor Woman
        assert!(
            names.iter().any(|n| n.contains("Techno Man")),
            "should contain Techno Man, got {:?}",
            names
        );
        assert!(
            names.iter().any(|n| n.contains("Bass Man")),
            "should contain Bass Man"
        );
        assert!(
            names.iter().any(|n| n.contains("Violin Woman")),
            "should contain Violin Woman"
        );
        assert!(
            names.iter().any(|n| n.contains("Conductor Woman")),
            "should contain Conductor Woman"
        );
        assert_eq!(names.len(), 4, "should have 4 artists, got {:?}", names);
    }

    // 4. readdir on a specific release returns track filenames and .rose.{id}.toml.
    #[test]
    fn test_readdir_release_tracks() {
        let (_dir, config) = seeded_config();
        let mut core = RoseLogicalCore::new(config);

        // First, readdir on /1. Releases to populate release name cache.
        let releases_vp = VirtualPath::parse(Path::new("/1. Releases")).unwrap();
        let release_entries = core.readdir(&releases_vp).unwrap();
        let release_names = entry_names(&release_entries);

        // Find the release name for r1 (Release 1). The template generates a name
        // that includes "Release 1" somewhere.
        let r1_name = release_names
            .iter()
            .find(|n| n.contains("Release 1"))
            .expect("should find Release 1 in readdir");

        // Now readdir on that specific release.
        let release_path = format!("/1. Releases/{}", r1_name);
        let vp = VirtualPath::parse(Path::new(&release_path)).unwrap();
        let entries = core.readdir(&vp).unwrap();
        let names = entry_names(&entries);

        // r1 has 2 tracks (t1, t2), no cover image, and a .rose.r1.toml datafile.
        assert!(
            names.iter().any(|n| n.contains(".rose.r1.toml")),
            "should contain .rose.r1.toml, got {:?}",
            names
        );
        // 2 tracks + 1 datafile = 3 entries (no cover art for r1)
        assert_eq!(
            names.len(),
            3,
            "r1 should have 2 tracks + 1 datafile, got {:?}",
            names
        );
    }

    // 5. readdir on /Collages returns collage names.
    #[test]
    fn test_readdir_collages_view() {
        let (_dir, config) = seeded_config();
        let mut core = RoseLogicalCore::new(config);

        let vp = VirtualPath::parse(Path::new("/7. Collages")).unwrap();
        let entries = core.readdir(&vp).unwrap();
        let names = entry_names(&entries);

        assert!(
            names.contains(&"Rose Gold".to_string()),
            "should contain Rose Gold"
        );
        assert!(
            names.contains(&"Ruby Red".to_string()),
            "should contain Ruby Red"
        );
        assert_eq!(names.len(), 2, "should have 2 collages");
    }

    // 6. readdir on /Playlists returns playlist names.
    #[test]
    fn test_readdir_playlists_view() {
        let (_dir, config) = seeded_config();
        let mut core = RoseLogicalCore::new(config);

        let vp = VirtualPath::parse(Path::new("/8. Playlists")).unwrap();
        let entries = core.readdir(&vp).unwrap();
        let names = entry_names(&entries);

        assert!(
            names.contains(&"Lala Lisa".to_string()),
            "should contain Lala Lisa"
        );
        assert!(
            names.contains(&"Turtle Rabbit".to_string()),
            "should contain Turtle Rabbit"
        );
        assert_eq!(names.len(), 2, "should have 2 playlists");
    }

    // -----------------------------------------------------------------------
    // getattr tests
    // -----------------------------------------------------------------------

    // 7. getattr on root returns directory attributes.
    #[test]
    fn test_getattr_root() {
        let (_dir, config) = seeded_config();
        let mut core = RoseLogicalCore::new(config);

        let root = VirtualPath::parse(Path::new("/")).unwrap();
        let attrs = core.getattr(&root).unwrap();

        // Root should be a directory.
        assert_ne!(
            attrs.st_mode & libc::S_IFDIR,
            0,
            "root should be a directory"
        );
    }

    // 8. getattr on a release directory returns directory attributes.
    #[test]
    fn test_getattr_release_dir() {
        let (_dir, config) = seeded_config();
        let mut core = RoseLogicalCore::new(config);

        // First populate release cache via readdir.
        let releases_vp = VirtualPath::parse(Path::new("/1. Releases")).unwrap();
        let release_entries = core.readdir(&releases_vp).unwrap();
        let release_names = entry_names(&release_entries);

        let r1_name = release_names
            .iter()
            .find(|n| n.contains("Release 1"))
            .expect("should find Release 1");

        let release_path = format!("/1. Releases/{}", r1_name);
        let vp = VirtualPath::parse(Path::new(&release_path)).unwrap();
        let attrs = core.getattr(&vp).unwrap();

        assert_ne!(
            attrs.st_mode & libc::S_IFDIR,
            0,
            "release dir should be a directory"
        );
    }

    // 9. getattr on a track file returns file attributes with non-zero size.
    #[test]
    fn test_getattr_track_file() {
        let (_dir, config) = seeded_config();
        let mut core = RoseLogicalCore::new(config);

        // Populate release + track caches via readdir.
        let releases_vp = VirtualPath::parse(Path::new("/1. Releases")).unwrap();
        let release_entries = core.readdir(&releases_vp).unwrap();
        let release_names = entry_names(&release_entries);

        let r1_name = release_names
            .iter()
            .find(|n| n.contains("Release 1"))
            .expect("should find Release 1");

        let release_path = format!("/1. Releases/{}", r1_name);
        let release_vp = VirtualPath::parse(Path::new(&release_path)).unwrap();
        let track_entries = core.readdir(&release_vp).unwrap();
        let track_names = entry_names(&track_entries);

        // Find a track file (not the .rose.*.toml datafile).
        let track_name = track_names
            .iter()
            .find(|n| !n.starts_with(".rose."))
            .expect("should find at least one track file");

        let track_path = format!("/1. Releases/{}/{}", r1_name, track_name);
        let vp = VirtualPath::parse(Path::new(&track_path)).unwrap();
        let attrs = core.getattr(&vp).unwrap();

        // Should be a file.
        assert_ne!(
            attrs.st_mode & libc::S_IFREG,
            0,
            "track should be a regular file"
        );
        // The actual source file exists (we created it), so st_size should be > 0.
        assert!(attrs.st_size > 0, "track file should have non-zero size");
    }

    // 10. getattr on a nonexistent path returns ENOENT.
    #[test]
    fn test_getattr_nonexistent() {
        let (_dir, config) = seeded_config();
        let mut core = RoseLogicalCore::new(config);

        // A completely unknown view path.
        let result = VirtualPath::parse(Path::new("/9. Nonexistent"));
        assert_eq!(result, Err(libc::ENOENT));

        // An artist that doesn't exist.
        let vp = VirtualPath {
            view: Some(ViewType::Artists),
            artist: Some("Nobody At All".to_string()),
            genre: None,
            descriptor: None,
            label: None,
            collage: None,
            playlist: None,
            release: None,
            file: None,
        };
        let result = core.getattr(&vp);
        assert!(
            matches!(result, Err(e) if e == libc::ENOENT),
            "nonexistent artist should return ENOENT, got {:?}",
            result.err()
        );
    }

    // -----------------------------------------------------------------------
    // Whitelist / blacklist integration tests
    // -----------------------------------------------------------------------

    // 11. readdir with artist blacklist: blacklisted artist doesn't appear.
    #[test]
    fn test_readdir_with_blacklist() {
        let (_dir, config) =
            seeded_config_with_vfs_options(None, Some(vec!["Techno Man".to_string()]));
        let mut core = RoseLogicalCore::new(config);

        let vp = VirtualPath::parse(Path::new("/2. Artists")).unwrap();
        let entries = core.readdir(&vp).unwrap();
        let names = entry_names(&entries);

        assert!(
            !names.iter().any(|n| n.contains("Techno Man")),
            "blacklisted artist 'Techno Man' should not appear, got {:?}",
            names
        );
        // Other artists should still be present.
        assert!(
            names.iter().any(|n| n.contains("Violin Woman")),
            "non-blacklisted artist should still appear"
        );
    }

    // 12. readdir with artist whitelist: only whitelisted artists appear.
    #[test]
    fn test_readdir_with_whitelist() {
        let (_dir, config) =
            seeded_config_with_vfs_options(Some(vec!["Violin Woman".to_string()]), None);
        let mut core = RoseLogicalCore::new(config);

        let vp = VirtualPath::parse(Path::new("/2. Artists")).unwrap();
        let entries = core.readdir(&vp).unwrap();
        let names = entry_names(&entries);

        assert_eq!(
            names.len(),
            1,
            "only whitelisted artist should appear, got {:?}",
            names
        );
        assert!(
            names.iter().any(|n| n.contains("Violin Woman")),
            "whitelisted artist 'Violin Woman' should appear"
        );
        assert!(
            !names.iter().any(|n| n.contains("Techno Man")),
            "non-whitelisted artist should not appear"
        );
    }

    // -----------------------------------------------------------------------
    // Mutation test helpers
    // -----------------------------------------------------------------------

    /// Set up a test environment with real on-disk collages and playlists
    /// that work with the mutation functions (create/delete/rename etc).
    /// Returns (TempDir, Config) — TempDir must be kept alive.
    fn seeded_mutation_config() -> (TempDir, Config) {
        let dir = TempDir::new().unwrap();
        let music_dir = dir.path().join("music");
        let cache_dir = dir.path().join("cache");
        let vfs_dir = dir.path().join("vfs");
        std::fs::create_dir_all(&music_dir).unwrap();
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::create_dir_all(&vfs_dir).unwrap();

        let cfg_path = dir.path().join("config.toml");
        let mut f = std::fs::File::create(&cfg_path).unwrap();
        write!(
            f,
            r#"
music_source_dir = "{}"
cache_dir = "{}"
vfs.mount_dir = "{}"
"#,
            music_dir.display(),
            cache_dir.display(),
            vfs_dir.display(),
        )
        .unwrap();
        let config = Config::parse(Some(&cfg_path)).unwrap();
        maybe_invalidate_cache_database(&config).unwrap();

        // Create release source directories on disk.
        let rel1_dir = music_dir.join("Release1");
        let rel2_dir = music_dir.join("Release2");
        std::fs::create_dir_all(&rel1_dir).unwrap();
        std::fs::create_dir_all(&rel2_dir).unwrap();

        // Create sidecar files.
        std::fs::write(rel1_dir.join(".rose.ilovecarly.toml"), "").unwrap();
        std::fs::write(rel2_dir.join(".rose.ilovenewjeans.toml"), "").unwrap();

        // Create fake audio files so stat works.
        std::fs::write(rel1_dir.join("track1.flac"), b"fake audio").unwrap();
        std::fs::write(rel2_dir.join("track2.flac"), b"fake audio").unwrap();

        // Insert releases and tracks into cache.
        let conn = connect(&config).unwrap();
        conn.execute_batch(&format!(
            r#"
INSERT INTO releases
       (id, source_path, cover_image_path, added_at, datafile_mtime,
        title, releasetype, releasedate, disctotal, new, favorite, metahash)
VALUES ('ilovecarly', '{}', NULL, '2024-01-01T00:00:00+00:00', '0',
        'Carly Rae Jepsen - 2015. E-MO-TION', 'album', '2024', 1, 1, 0, 'hash1'),
       ('ilovenewjeans', '{}', NULL, '2024-01-01T00:00:00+00:00', '0',
        'NewJeans - 2023. Get Up', 'album', '2023', 1, 1, 0, 'hash2');

INSERT INTO releases_artists
       (release_id, artist, role, position)
VALUES ('ilovecarly', 'Carly Rae Jepsen', 'main', 0),
       ('ilovenewjeans', 'NewJeans', 'main', 0);

INSERT INTO tracks
       (id, source_path, source_mtime, title, release_id,
        tracknumber, tracktotal, discnumber, duration_seconds, metahash)
VALUES ('trackA', '{}', '0', 'Track One', 'ilovecarly',
        '1', 10, '1', 120, 'thash1'),
       ('trackB', '{}', '0', 'Track Two', 'ilovenewjeans',
        '1', 10, '1', 180, 'thash2');

INSERT INTO tracks_artists
       (track_id, artist, role, position)
VALUES ('trackA', 'Carly Rae Jepsen', 'main', 0),
       ('trackB', 'NewJeans', 'main', 0);
"#,
            rel1_dir.display(),
            rel2_dir.display(),
            rel1_dir.join("track1.flac").display(),
            rel2_dir.join("track2.flac").display(),
        ))
        .expect("Failed to seed mutation test database");
        drop(conn);

        // Create collage "Rose Gold" with both releases.
        let collages_dir = music_dir.join("!collages");
        std::fs::create_dir_all(&collages_dir).unwrap();
        std::fs::write(
            collages_dir.join("Rose Gold.toml"),
            r#"[[releases]]
uuid = "ilovecarly"
description_meta = "Carly Rae Jepsen - 2015. E-MO-TION"

[[releases]]
uuid = "ilovenewjeans"
description_meta = "NewJeans - 2023. Get Up"
"#,
        )
        .unwrap();
        rose_core::cache::update_cache_for_collages(
            &config,
            Some(vec!["Rose Gold".to_string()]),
            true,
        )
        .unwrap();

        // Create playlist "Lala Lisa" with both tracks.
        let playlists_dir = music_dir.join("!playlists");
        std::fs::create_dir_all(&playlists_dir).unwrap();
        std::fs::write(
            playlists_dir.join("Lala Lisa.toml"),
            r#"[[tracks]]
uuid = "trackA"
description_meta = "Carly Rae Jepsen - Track One"

[[tracks]]
uuid = "trackB"
description_meta = "NewJeans - Track Two"
"#,
        )
        .unwrap();
        rose_core::cache::update_cache_for_playlists(
            &config,
            Some(vec!["Lala Lisa".to_string()]),
            true,
        )
        .unwrap();

        (dir, config)
    }

    // -----------------------------------------------------------------------
    // Collage mutation tests
    // -----------------------------------------------------------------------

    // 13. mkdir at /Collages/New Collage creates a collage TOML on disk.
    #[test]
    fn test_mkdir_creates_collage() {
        let (_dir, config) = seeded_mutation_config();
        let mut core = RoseLogicalCore::new(config.clone());

        let vp = VirtualPath {
            view: Some(ViewType::Collages),
            collage: Some("New Collage".to_string()),
            artist: None,
            genre: None,
            descriptor: None,
            label: None,
            playlist: None,
            release: None,
            file: None,
        };

        core.mkdir(&vp).unwrap();

        // Assert collage TOML file was created on disk.
        let path = rose_core::collages::collage_path(&config, "New Collage");
        assert!(path.exists(), "Collage TOML should exist at {:?}", path);

        // Also verify it's in the cache.
        let conn = connect(&config).unwrap();
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT * FROM collages WHERE name = 'New Collage')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(exists, "Collage should exist in cache");
    }

    // 14. rmdir at /Collages/{existing} deletes the collage.
    #[test]
    fn test_rmdir_deletes_collage() {
        let (_dir, config) = seeded_mutation_config();
        let mut core = RoseLogicalCore::new(config.clone());

        let path = rose_core::collages::collage_path(&config, "Rose Gold");
        assert!(path.exists(), "Collage should exist before deletion");

        let vp = VirtualPath {
            view: Some(ViewType::Collages),
            collage: Some("Rose Gold".to_string()),
            artist: None,
            genre: None,
            descriptor: None,
            label: None,
            playlist: None,
            release: None,
            file: None,
        };

        core.rmdir(&vp).unwrap();

        assert!(!path.exists(), "Collage TOML should be deleted/trashed");

        let conn = connect(&config).unwrap();
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT * FROM collages WHERE name = 'Rose Gold')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!exists, "Collage should not exist in cache after deletion");
    }

    // 15. rename /Collages/Old to /Collages/New renames the collage on disk.
    #[test]
    fn test_rename_collage() {
        let (_dir, config) = seeded_mutation_config();
        let mut core = RoseLogicalCore::new(config.clone());

        let old_vp = VirtualPath {
            view: Some(ViewType::Collages),
            collage: Some("Rose Gold".to_string()),
            artist: None,
            genre: None,
            descriptor: None,
            label: None,
            playlist: None,
            release: None,
            file: None,
        };
        let new_vp = VirtualPath {
            view: Some(ViewType::Collages),
            collage: Some("Silver".to_string()),
            artist: None,
            genre: None,
            descriptor: None,
            label: None,
            playlist: None,
            release: None,
            file: None,
        };

        core.rename(&old_vp, &new_vp).unwrap();

        let old_path = rose_core::collages::collage_path(&config, "Rose Gold");
        let new_path = rose_core::collages::collage_path(&config, "Silver");
        assert!(!old_path.exists(), "Old collage file should be gone");
        assert!(new_path.exists(), "New collage file should exist");

        let conn = connect(&config).unwrap();
        let old_exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT * FROM collages WHERE name = 'Rose Gold')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let new_exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT * FROM collages WHERE name = 'Silver')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!old_exists, "Old collage should not be in cache");
        assert!(new_exists, "New collage should be in cache");
    }

    // 16. open .rose.{uuid}.toml under a collage release adds release to collage.
    #[test]
    fn test_open_adds_release_to_collage() {
        let (_dir, config) = seeded_mutation_config();

        // Create a new empty collage for the test.
        rose_core::collages::create_collage(&config, "TestAdd").unwrap();

        let mut core = RoseLogicalCore::new(config.clone());

        // Simulate the FUSE collage addition sequence:
        // open a `.rose.{release_id}.toml` file under a collage/release path with O_CREAT.
        let vp = VirtualPath {
            view: Some(ViewType::Collages),
            collage: Some("TestAdd".to_string()),
            release: Some("SomeRelease".to_string()),
            file: Some(".rose.ilovecarly.toml".to_string()),
            artist: None,
            genre: None,
            descriptor: None,
            label: None,
            playlist: None,
        };

        let fh = core.open(&vp, libc::O_CREAT | libc::O_WRONLY).unwrap();
        // The returned fh should be the dev_null sentinel for collage additions.
        assert_eq!(fh, core.fhandler.dev_null);

        // Assert the release was added to the collage TOML.
        let filepath = rose_core::collages::collage_path(&config, "TestAdd");
        let content = std::fs::read_to_string(&filepath).unwrap();
        let data: toml::Value = content.parse().unwrap();
        let releases = data["releases"].as_array().unwrap();
        let uuids: Vec<&str> = releases
            .iter()
            .map(|r| r["uuid"].as_str().unwrap())
            .collect();
        assert!(
            uuids.contains(&"ilovecarly"),
            "Release should be added to collage TOML, got {:?}",
            uuids
        );
    }

    // -----------------------------------------------------------------------
    // Playlist mutation tests
    // -----------------------------------------------------------------------

    // 17. mkdir at /Playlists/New Playlist creates a playlist TOML on disk.
    #[test]
    fn test_mkdir_creates_playlist() {
        let (_dir, config) = seeded_mutation_config();
        let mut core = RoseLogicalCore::new(config.clone());

        let vp = VirtualPath {
            view: Some(ViewType::Playlists),
            playlist: Some("New Playlist".to_string()),
            artist: None,
            genre: None,
            descriptor: None,
            label: None,
            collage: None,
            release: None,
            file: None,
        };

        core.mkdir(&vp).unwrap();

        let path = rose_core::playlists::playlist_path(&config, "New Playlist");
        assert!(path.exists(), "Playlist TOML should exist at {:?}", path);

        let conn = connect(&config).unwrap();
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT * FROM playlists WHERE name = 'New Playlist')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(exists, "Playlist should exist in cache");
    }

    // 18. rmdir at /Playlists/{existing} deletes the playlist.
    #[test]
    fn test_rmdir_deletes_playlist() {
        let (_dir, config) = seeded_mutation_config();
        let mut core = RoseLogicalCore::new(config.clone());

        let path = rose_core::playlists::playlist_path(&config, "Lala Lisa");
        assert!(path.exists(), "Playlist should exist before deletion");

        let vp = VirtualPath {
            view: Some(ViewType::Playlists),
            playlist: Some("Lala Lisa".to_string()),
            artist: None,
            genre: None,
            descriptor: None,
            label: None,
            collage: None,
            release: None,
            file: None,
        };

        core.rmdir(&vp).unwrap();

        assert!(!path.exists(), "Playlist TOML should be deleted/trashed");

        let conn = connect(&config).unwrap();
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT * FROM playlists WHERE name = 'Lala Lisa')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!exists, "Playlist should not exist in cache after deletion");
    }

    // 19. rename /Playlists/Old to /Playlists/New renames on disk.
    #[test]
    fn test_rename_playlist() {
        let (_dir, config) = seeded_mutation_config();
        let mut core = RoseLogicalCore::new(config.clone());

        let old_vp = VirtualPath {
            view: Some(ViewType::Playlists),
            playlist: Some("Lala Lisa".to_string()),
            artist: None,
            genre: None,
            descriptor: None,
            label: None,
            collage: None,
            release: None,
            file: None,
        };
        let new_vp = VirtualPath {
            view: Some(ViewType::Playlists),
            playlist: Some("Turtle Rabbit".to_string()),
            artist: None,
            genre: None,
            descriptor: None,
            label: None,
            collage: None,
            release: None,
            file: None,
        };

        core.rename(&old_vp, &new_vp).unwrap();

        let old_path = rose_core::playlists::playlist_path(&config, "Lala Lisa");
        let new_path = rose_core::playlists::playlist_path(&config, "Turtle Rabbit");
        assert!(!old_path.exists(), "Old playlist file should be gone");
        assert!(new_path.exists(), "New playlist file should exist");

        let conn = connect(&config).unwrap();
        let old_exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT * FROM playlists WHERE name = 'Lala Lisa')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let new_exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT * FROM playlists WHERE name = 'Turtle Rabbit')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!old_exists, "Old playlist should not be in cache");
        assert!(new_exists, "New playlist should be in cache");
    }

    // 20. unlink a track file under a playlist path removes it from the TOML.
    #[test]
    fn test_unlink_removes_track_from_playlist() {
        let (_dir, config) = seeded_mutation_config();
        let mut core = RoseLogicalCore::new(config.clone());

        // Readdir on the playlist to populate the vnames cache.
        let playlist_vp = VirtualPath {
            view: Some(ViewType::Playlists),
            playlist: Some("Lala Lisa".to_string()),
            artist: None,
            genre: None,
            descriptor: None,
            label: None,
            collage: None,
            release: None,
            file: None,
        };
        let entries = core.readdir(&playlist_vp).unwrap();
        let names = entry_names(&entries);

        // Find the virtual file name for trackA (Track One).
        let track_file = names
            .iter()
            .find(|name| name.contains("Track One"))
            .expect("Should find Track One in playlist entries")
            .clone();

        // Unlink that track.
        let unlink_vp = VirtualPath {
            view: Some(ViewType::Playlists),
            playlist: Some("Lala Lisa".to_string()),
            file: Some(track_file),
            artist: None,
            genre: None,
            descriptor: None,
            label: None,
            collage: None,
            release: None,
        };

        core.unlink(&unlink_vp).unwrap();

        // Assert track was removed from the playlist TOML.
        let filepath = rose_core::playlists::playlist_path(&config, "Lala Lisa");
        let content = std::fs::read_to_string(&filepath).unwrap();
        let data: toml::Value = content.parse().unwrap();
        let tracks = data["tracks"].as_array().unwrap();
        let uuids: Vec<&str> = tracks.iter().map(|t| t["uuid"].as_str().unwrap()).collect();
        assert!(
            !uuids.contains(&"trackA"),
            "trackA should be removed from playlist, got {:?}",
            uuids
        );
        assert!(
            uuids.contains(&"trackB"),
            "trackB should still be in playlist"
        );
    }

    // -----------------------------------------------------------------------
    // Release mutation tests
    // -----------------------------------------------------------------------

    // 21. rmdir at /Releases/{release} trashes the release source directory.
    #[test]
    fn test_rmdir_deletes_release() {
        let (_dir, config) = seeded_mutation_config();
        let mut core = RoseLogicalCore::new(config.clone());

        // Readdir /1. Releases to populate release name cache.
        let releases_vp = VirtualPath::parse(Path::new("/1. Releases")).unwrap();
        let entries = core.readdir(&releases_vp).unwrap();
        let names = entry_names(&entries);

        // Find the virtual name for "ilovecarly" (contains "E-MO-TION" or "Carly").
        let release_name = names
            .iter()
            .find(|n| n.contains("Carly") || n.contains("E-MO-TION"))
            .expect("Should find Carly Rae Jepsen release in readdir")
            .clone();

        let rmdir_vp = VirtualPath {
            view: Some(ViewType::Releases),
            release: Some(release_name),
            artist: None,
            genre: None,
            descriptor: None,
            label: None,
            collage: None,
            playlist: None,
            file: None,
        };

        let rel_dir = config.music_source_dir.join("Release1");
        assert!(
            rel_dir.exists(),
            "Release source dir should exist before rmdir"
        );

        core.rmdir(&rmdir_vp).unwrap();

        assert!(
            !rel_dir.exists(),
            "Release source dir should be trashed after rmdir"
        );
    }

    // -----------------------------------------------------------------------
    // Cover art mutation tests
    // -----------------------------------------------------------------------

    // 22. Open, write, release cover art for a release creates the file.
    #[test]
    fn test_write_release_cover_art() {
        let (_dir, config) = seeded_mutation_config();
        let mut core = RoseLogicalCore::new(config.clone());

        // Readdir /1. Releases to populate the release name cache.
        let releases_vp = VirtualPath::parse(Path::new("/1. Releases")).unwrap();
        let entries = core.readdir(&releases_vp).unwrap();
        let names = entry_names(&entries);

        let release_name = names
            .iter()
            .find(|n| n.contains("Carly") || n.contains("E-MO-TION"))
            .expect("Should find release in readdir")
            .clone();

        // Open cover art for writing with O_CREAT.
        let cover_vp = VirtualPath {
            view: Some(ViewType::Releases),
            release: Some(release_name),
            file: Some("cover.jpg".to_string()),
            artist: None,
            genre: None,
            descriptor: None,
            label: None,
            collage: None,
            playlist: None,
        };

        let fh = core
            .open(&cover_vp, libc::O_CREAT | libc::O_WRONLY)
            .unwrap();

        // Write some image data.
        let fake_image_data = b"FAKE_JPEG_DATA_12345";
        let written = core.write(fh, 0, fake_image_data).unwrap();
        assert_eq!(written, fake_image_data.len() as u32);

        // Release the handle — triggers the actual cover art copy.
        core.release(fh).unwrap();

        // Assert cover art was created in the release directory.
        let cover_path = config.music_source_dir.join("Release1").join("cover.jpg");
        assert!(
            cover_path.exists(),
            "Cover art should exist at {:?}",
            cover_path
        );

        // Verify the contents match.
        let contents = std::fs::read(&cover_path).unwrap();
        assert_eq!(contents, fake_image_data);
    }
}
