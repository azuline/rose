//! PyO3 Python extension module for Rose.
//!
//! Provides thin wrappers around `rose-core` types and functions so that
//! `import rose` works from Python scripts.

#![allow(clippy::useless_conversion)] // PyO3 macro expansions trigger this

use std::path::Path;

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyType;

// ---------------------------------------------------------------------------
// Error conversion
// ---------------------------------------------------------------------------

/// Convert a `rose_core::common::RoseError` into a Python `RuntimeError`.
fn to_pyerr(e: rose_core::common::RoseError) -> PyErr {
    PyRuntimeError::new_err(e.to_string())
}

// ===========================================================================
// Task 044: Config
// ===========================================================================

/// Python wrapper for `rose_core::config::Config`.
#[pyclass(name = "Config")]
pub struct PyConfig {
    inner: rose_core::config::Config,
}

#[pymethods]
impl PyConfig {
    /// Load configuration from a TOML file.
    ///
    /// If *path* is ``None``, the default XDG config path is used.
    #[classmethod]
    #[pyo3(signature = (path=None))]
    fn load(_cls: &Bound<'_, PyType>, path: Option<&str>) -> PyResult<Self> {
        let p = path.map(std::path::PathBuf::from);
        let config = rose_core::config::Config::parse(p.as_deref()).map_err(to_pyerr)?;
        Ok(PyConfig { inner: config })
    }

    #[getter]
    fn music_source_dir(&self) -> String {
        self.inner.music_source_dir.to_string_lossy().into_owned()
    }

    #[getter]
    fn fuse_mount_dir(&self) -> String {
        self.inner.vfs.mount_dir.to_string_lossy().into_owned()
    }

    #[getter]
    fn cache_dir(&self) -> String {
        self.inner.cache_dir.to_string_lossy().into_owned()
    }

    #[getter]
    fn max_proc(&self) -> usize {
        self.inner.max_proc
    }

    #[getter]
    fn rename_source_files(&self) -> bool {
        self.inner.rename_source_files
    }

    #[getter]
    fn max_filename_bytes(&self) -> usize {
        self.inner.max_filename_bytes
    }

    #[getter]
    fn cover_art_stems(&self) -> Vec<String> {
        self.inner.cover_art_stems.clone()
    }

    #[getter]
    fn valid_art_exts(&self) -> Vec<String> {
        self.inner.valid_art_exts.clone()
    }

    #[getter]
    fn write_parent_genres(&self) -> bool {
        self.inner.write_parent_genres
    }

    #[getter]
    fn ignore_release_directories(&self) -> Vec<String> {
        self.inner.ignore_release_directories.clone()
    }
}

// ===========================================================================
// Task 045: Read types
// ===========================================================================

// ---------------------------------------------------------------------------
// Artist helpers
// ---------------------------------------------------------------------------

/// Serialize an `ArtistMapping` to a Python-friendly dict representation.
fn artist_mapping_to_py(m: &rose_core::common::ArtistMapping) -> Vec<(String, Vec<String>)> {
    m.items()
        .into_iter()
        .map(|(role, artists)| {
            (
                role.to_string(),
                artists.iter().map(|a| a.name.clone()).collect(),
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Release
// ---------------------------------------------------------------------------

#[pyclass(name = "Release")]
pub struct PyRelease {
    inner: rose_core::cache::Release,
}

#[pymethods]
impl PyRelease {
    #[getter]
    fn id(&self) -> &str {
        &self.inner.id
    }

    #[getter]
    fn releasetitle(&self) -> &str {
        &self.inner.releasetitle
    }

    #[getter]
    fn releasedate(&self) -> Option<String> {
        self.inner.releasedate.as_ref().map(|d| d.to_string())
    }

    #[getter]
    fn originaldate(&self) -> Option<String> {
        self.inner.originaldate.as_ref().map(|d| d.to_string())
    }

    #[getter]
    fn compositiondate(&self) -> Option<String> {
        self.inner.compositiondate.as_ref().map(|d| d.to_string())
    }

    #[getter]
    fn edition(&self) -> Option<&str> {
        self.inner.edition.as_deref()
    }

    #[getter]
    fn catalognumber(&self) -> Option<&str> {
        self.inner.catalognumber.as_deref()
    }

    #[getter]
    fn releasetype(&self) -> &str {
        &self.inner.releasetype
    }

    #[getter]
    fn genres(&self) -> Vec<String> {
        self.inner.genres.clone()
    }

    #[getter]
    fn secondary_genres(&self) -> Vec<String> {
        self.inner.secondary_genres.clone()
    }

    #[getter]
    fn descriptors(&self) -> Vec<String> {
        self.inner.descriptors.clone()
    }

    #[getter]
    fn labels(&self) -> Vec<String> {
        self.inner.labels.clone()
    }

    #[getter(new)]
    #[allow(clippy::wrong_self_convention, clippy::new_ret_no_self)]
    fn is_new(&self) -> bool {
        self.inner.new
    }

    #[getter]
    fn favorite(&self) -> bool {
        self.inner.favorite
    }

    #[getter]
    fn rating(&self) -> Option<i32> {
        self.inner.rating
    }

    #[getter]
    fn disctotal(&self) -> i32 {
        self.inner.disctotal
    }

    #[getter]
    fn source_path(&self) -> String {
        self.inner.source_path.to_string_lossy().into_owned()
    }

    #[getter]
    fn cover_image_path(&self) -> Option<String> {
        self.inner
            .cover_image_path
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
    }

    #[getter]
    fn added_at(&self) -> &str {
        &self.inner.added_at
    }

    #[getter]
    fn releaseartists(&self) -> Vec<(String, Vec<String>)> {
        artist_mapping_to_py(&self.inner.releaseartists)
    }

    fn __repr__(&self) -> String {
        format!(
            "Release(id={:?}, title={:?})",
            self.inner.id, self.inner.releasetitle
        )
    }
}

// ---------------------------------------------------------------------------
// Track
// ---------------------------------------------------------------------------

#[pyclass(name = "Track")]
pub struct PyTrack {
    inner: rose_core::cache::Track,
}

#[pymethods]
impl PyTrack {
    #[getter]
    fn id(&self) -> &str {
        &self.inner.id
    }

    #[getter]
    fn tracktitle(&self) -> &str {
        &self.inner.tracktitle
    }

    #[getter]
    fn tracknumber(&self) -> &str {
        &self.inner.tracknumber
    }

    #[getter]
    fn discnumber(&self) -> &str {
        &self.inner.discnumber
    }

    #[getter]
    fn duration_seconds(&self) -> i32 {
        self.inner.duration_seconds
    }

    #[getter]
    fn source_path(&self) -> String {
        self.inner.source_path.to_string_lossy().into_owned()
    }

    #[getter]
    fn trackartists(&self) -> Vec<(String, Vec<String>)> {
        artist_mapping_to_py(&self.inner.trackartists)
    }

    #[getter]
    fn release(&self) -> PyRelease {
        PyRelease {
            inner: self.inner.release.clone(),
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "Track(id={:?}, title={:?})",
            self.inner.id, self.inner.tracktitle
        )
    }
}

// ---------------------------------------------------------------------------
// Collage
// ---------------------------------------------------------------------------

#[pyclass(name = "Collage")]
pub struct PyCollage {
    name: String,
}

#[pymethods]
impl PyCollage {
    #[getter]
    fn name(&self) -> &str {
        &self.name
    }

    fn __repr__(&self) -> String {
        format!("Collage(name={:?})", self.name)
    }
}

// ---------------------------------------------------------------------------
// Playlist
// ---------------------------------------------------------------------------

#[pyclass(name = "Playlist")]
pub struct PyPlaylist {
    name: String,
    cover_path: Option<String>,
}

#[pymethods]
impl PyPlaylist {
    #[getter]
    fn name(&self) -> &str {
        &self.name
    }

    #[getter]
    fn cover_path(&self) -> Option<&str> {
        self.cover_path.as_deref()
    }

    fn __repr__(&self) -> String {
        format!("Playlist(name={:?})", self.name)
    }
}

// ===========================================================================
// Task 045: Read functions
// ===========================================================================

/// Run a full cache update (must be called before reads return data).
#[pyfunction]
#[pyo3(signature = (config, force=None))]
fn update_cache(config: &PyConfig, force: Option<bool>) -> PyResult<()> {
    rose_core::cache::maybe_invalidate_cache_database(&config.inner).map_err(to_pyerr)?;
    rose_core::cache::update_cache(&config.inner, force.unwrap_or(false)).map_err(to_pyerr)
}

#[pyfunction]
fn list_releases(config: &PyConfig) -> PyResult<Vec<PyRelease>> {
    let releases = rose_core::cache::list_releases(&config.inner, None, true).map_err(to_pyerr)?;
    Ok(releases
        .into_iter()
        .map(|r| PyRelease { inner: r })
        .collect())
}

#[pyfunction]
fn list_tracks(config: &PyConfig) -> PyResult<Vec<PyTrack>> {
    let tracks = rose_core::cache::list_tracks(&config.inner, None).map_err(to_pyerr)?;
    Ok(tracks.into_iter().map(|t| PyTrack { inner: t }).collect())
}

#[pyfunction]
fn list_collages(config: &PyConfig) -> PyResult<Vec<String>> {
    rose_core::cache::list_collages(&config.inner).map_err(to_pyerr)
}

#[pyfunction]
fn list_playlists(config: &PyConfig) -> PyResult<Vec<String>> {
    rose_core::cache::list_playlists(&config.inner).map_err(to_pyerr)
}

#[pyfunction]
fn list_artists(config: &PyConfig) -> PyResult<Vec<String>> {
    rose_core::cache::list_artists(&config.inner).map_err(to_pyerr)
}

#[pyfunction]
fn list_genres(config: &PyConfig) -> PyResult<Vec<String>> {
    let entries = rose_core::cache::list_genres(&config.inner).map_err(to_pyerr)?;
    Ok(entries.into_iter().map(|e| e.genre).collect())
}

#[pyfunction]
fn list_labels(config: &PyConfig) -> PyResult<Vec<String>> {
    let entries = rose_core::cache::list_labels(&config.inner).map_err(to_pyerr)?;
    Ok(entries.into_iter().map(|e| e.label).collect())
}

#[pyfunction]
fn list_descriptors(config: &PyConfig) -> PyResult<Vec<String>> {
    let entries = rose_core::cache::list_descriptors(&config.inner).map_err(to_pyerr)?;
    Ok(entries.into_iter().map(|e| e.descriptor).collect())
}

#[pyfunction]
fn get_release(config: &PyConfig, release_id: &str) -> PyResult<Option<PyRelease>> {
    let release = rose_core::cache::get_release(&config.inner, release_id).map_err(to_pyerr)?;
    Ok(release.map(|r| PyRelease { inner: r }))
}

#[pyfunction]
fn get_track(config: &PyConfig, track_id: &str) -> PyResult<Option<PyTrack>> {
    let track = rose_core::cache::get_track(&config.inner, track_id).map_err(to_pyerr)?;
    Ok(track.map(|t| PyTrack { inner: t }))
}

#[pyfunction]
fn get_collage(config: &PyConfig, name: &str) -> PyResult<Option<PyCollage>> {
    let collage = rose_core::cache::get_collage(&config.inner, name).map_err(to_pyerr)?;
    Ok(collage.map(|c| PyCollage { name: c.name }))
}

#[pyfunction]
fn get_playlist(config: &PyConfig, name: &str) -> PyResult<Option<PyPlaylist>> {
    let playlist = rose_core::cache::get_playlist(&config.inner, name).map_err(to_pyerr)?;
    Ok(playlist.map(|p| PyPlaylist {
        name: p.name,
        cover_path: p.cover_path.map(|cp| cp.to_string_lossy().into_owned()),
    }))
}

// ===========================================================================
// Task 046: AudioTags
// ===========================================================================

#[pyclass(name = "AudioTags")]
pub struct PyAudioTags {
    inner: rose_core::audiotags::AudioTags,
}

#[pymethods]
impl PyAudioTags {
    /// Read tags from an audio file on disk.
    #[classmethod]
    fn from_file(_cls: &Bound<'_, PyType>, path: &str) -> PyResult<Self> {
        let tags = rose_core::audiotags::AudioTags::from_file(Path::new(path)).map_err(to_pyerr)?;
        Ok(PyAudioTags { inner: tags })
    }

    /// Write modified tags back to disk.
    fn flush(&mut self, config: &PyConfig) -> PyResult<()> {
        self.inner
            .flush(config.inner.write_parent_genres)
            .map_err(to_pyerr)
    }

    // -- Getters / setters --

    #[getter]
    fn tracktitle(&self) -> Option<&str> {
        self.inner.tracktitle.as_deref()
    }
    #[setter]
    fn set_tracktitle(&mut self, val: Option<String>) {
        self.inner.tracktitle = val;
    }

    #[getter]
    fn releasetitle(&self) -> Option<&str> {
        self.inner.releasetitle.as_deref()
    }
    #[setter]
    fn set_releasetitle(&mut self, val: Option<String>) {
        self.inner.releasetitle = val;
    }

    #[getter]
    fn tracknumber(&self) -> Option<&str> {
        self.inner.tracknumber.as_deref()
    }
    #[setter]
    fn set_tracknumber(&mut self, val: Option<String>) {
        self.inner.tracknumber = val;
    }

    #[getter]
    fn discnumber(&self) -> Option<&str> {
        self.inner.discnumber.as_deref()
    }
    #[setter]
    fn set_discnumber(&mut self, val: Option<String>) {
        self.inner.discnumber = val;
    }

    #[getter]
    fn duration_seconds(&self) -> i64 {
        self.inner.duration_sec
    }

    #[getter]
    fn releasetype(&self) -> &str {
        &self.inner.releasetype
    }
    #[setter]
    fn set_releasetype(&mut self, val: String) {
        self.inner.releasetype = val;
    }

    #[getter]
    fn releasedate(&self) -> Option<String> {
        self.inner.releasedate.as_ref().map(|d| d.to_string())
    }

    #[getter]
    fn genres(&self) -> Vec<String> {
        self.inner.genre.clone()
    }
    #[setter]
    fn set_genres(&mut self, val: Vec<String>) {
        self.inner.genre = val;
    }

    #[getter]
    fn secondary_genres(&self) -> Vec<String> {
        self.inner.secondarygenre.clone()
    }
    #[setter]
    fn set_secondary_genres(&mut self, val: Vec<String>) {
        self.inner.secondarygenre = val;
    }

    #[getter]
    fn descriptors(&self) -> Vec<String> {
        self.inner.descriptor.clone()
    }
    #[setter]
    fn set_descriptors(&mut self, val: Vec<String>) {
        self.inner.descriptor = val;
    }

    #[getter]
    fn labels(&self) -> Vec<String> {
        self.inner.label.clone()
    }
    #[setter]
    fn set_labels(&mut self, val: Vec<String>) {
        self.inner.label = val;
    }

    #[getter]
    fn edition(&self) -> Option<&str> {
        self.inner.edition.as_deref()
    }
    #[setter]
    fn set_edition(&mut self, val: Option<String>) {
        self.inner.edition = val;
    }

    #[getter]
    fn catalognumber(&self) -> Option<&str> {
        self.inner.catalognumber.as_deref()
    }
    #[setter]
    fn set_catalognumber(&mut self, val: Option<String>) {
        self.inner.catalognumber = val;
    }

    #[getter]
    fn path(&self) -> String {
        self.inner.path.to_string_lossy().into_owned()
    }

    fn __repr__(&self) -> String {
        format!(
            "AudioTags(path={:?}, title={:?})",
            self.inner.path.display(),
            self.inner.tracktitle
        )
    }
}

// ===========================================================================
// Task 046: Release write operations
// ===========================================================================

#[pyfunction]
fn toggle_release_new(config: &PyConfig, release_id: &str) -> PyResult<()> {
    rose_core::releases::toggle_release_new(&config.inner, release_id).map_err(to_pyerr)
}

#[pyfunction]
fn toggle_release_favorite(config: &PyConfig, release_id: &str) -> PyResult<()> {
    rose_core::releases::toggle_release_favorite(&config.inner, release_id).map_err(to_pyerr)
}

#[pyfunction]
#[pyo3(signature = (config, release_id, rating=None))]
fn set_release_rating(config: &PyConfig, release_id: &str, rating: Option<u8>) -> PyResult<()> {
    rose_core::releases::set_release_rating(&config.inner, release_id, rating).map_err(to_pyerr)
}

#[pyfunction]
fn delete_release(config: &PyConfig, release_id: &str) -> PyResult<()> {
    rose_core::releases::delete_release(&config.inner, release_id).map_err(to_pyerr)
}

#[pyfunction]
fn set_release_cover_art(config: &PyConfig, release_id: &str, path: &str) -> PyResult<()> {
    rose_core::releases::set_release_cover_art(&config.inner, release_id, Path::new(path))
        .map_err(to_pyerr)
}

#[pyfunction]
fn delete_release_cover_art(config: &PyConfig, release_id: &str) -> PyResult<()> {
    rose_core::releases::delete_release_cover_art(&config.inner, release_id).map_err(to_pyerr)
}

#[pyfunction]
#[pyo3(signature = (config, track_path, releasetype=None))]
fn create_single_release(
    config: &PyConfig,
    track_path: &str,
    releasetype: Option<&str>,
) -> PyResult<()> {
    rose_core::releases::create_single_release(
        &config.inner,
        Path::new(track_path),
        releasetype.unwrap_or("single"),
    )
    .map_err(to_pyerr)
}

// ===========================================================================
// Task 046: Collage write operations
// ===========================================================================

#[pyfunction]
fn create_collage(config: &PyConfig, name: &str) -> PyResult<()> {
    rose_core::collages::create_collage(&config.inner, name).map_err(to_pyerr)
}

#[pyfunction]
fn rename_collage(config: &PyConfig, old_name: &str, new_name: &str) -> PyResult<()> {
    rose_core::collages::rename_collage(&config.inner, old_name, new_name).map_err(to_pyerr)
}

#[pyfunction]
fn delete_collage(config: &PyConfig, name: &str) -> PyResult<()> {
    rose_core::collages::delete_collage(&config.inner, name).map_err(to_pyerr)
}

#[pyfunction]
fn add_release_to_collage(config: &PyConfig, collage_name: &str, release_id: &str) -> PyResult<()> {
    rose_core::collages::add_release_to_collage(&config.inner, collage_name, release_id)
        .map_err(to_pyerr)
}

#[pyfunction]
fn remove_release_from_collage(
    config: &PyConfig,
    collage_name: &str,
    release_id: &str,
) -> PyResult<()> {
    rose_core::collages::remove_release_from_collage(&config.inner, collage_name, release_id)
        .map_err(to_pyerr)
}

// ===========================================================================
// Task 046: Playlist write operations
// ===========================================================================

#[pyfunction]
fn create_playlist(config: &PyConfig, name: &str) -> PyResult<()> {
    rose_core::playlists::create_playlist(&config.inner, name).map_err(to_pyerr)
}

#[pyfunction]
fn rename_playlist(config: &PyConfig, old_name: &str, new_name: &str) -> PyResult<()> {
    rose_core::playlists::rename_playlist(&config.inner, old_name, new_name).map_err(to_pyerr)
}

#[pyfunction]
fn delete_playlist(config: &PyConfig, name: &str) -> PyResult<()> {
    rose_core::playlists::delete_playlist(&config.inner, name).map_err(to_pyerr)
}

#[pyfunction]
fn add_track_to_playlist(config: &PyConfig, playlist_name: &str, track_id: &str) -> PyResult<()> {
    rose_core::playlists::add_track_to_playlist(&config.inner, playlist_name, track_id)
        .map_err(to_pyerr)
}

#[pyfunction]
fn remove_track_from_playlist(
    config: &PyConfig,
    playlist_name: &str,
    track_id: &str,
) -> PyResult<()> {
    rose_core::playlists::remove_track_from_playlist(&config.inner, playlist_name, track_id)
        .map_err(to_pyerr)
}

#[pyfunction]
fn set_playlist_cover_art(config: &PyConfig, playlist_name: &str, path: &str) -> PyResult<()> {
    rose_core::playlists::set_playlist_cover_art(&config.inner, playlist_name, Path::new(path))
        .map_err(to_pyerr)
}

#[pyfunction]
fn delete_playlist_cover_art(config: &PyConfig, playlist_name: &str) -> PyResult<()> {
    rose_core::playlists::delete_playlist_cover_art(&config.inner, playlist_name).map_err(to_pyerr)
}

// ===========================================================================
// Task 046: Rule execution
// ===========================================================================

/// Execute all stored metadata rules from the configuration.
///
/// *dry_run*: if true, print changes without writing them.
/// *confirm_yes*: if true, skip interactive confirmation prompts.
#[pyfunction]
#[pyo3(signature = (config, dry_run=None, confirm_yes=None))]
fn execute_stored_metadata_rules(
    config: &PyConfig,
    dry_run: Option<bool>,
    confirm_yes: Option<bool>,
) -> PyResult<()> {
    rose_core::rules::execute_stored_metadata_rules(
        &config.inner,
        dry_run.unwrap_or(false),
        confirm_yes.unwrap_or(true),
    )
    .map_err(to_pyerr)
}

/// Execute a single metadata rule specified by DSL strings.
///
/// *matcher*: matcher DSL string, e.g. ``"tracktitle:hello"``.
/// *actions*: list of action DSL strings, e.g. ``["replace:world"]``.
/// *ignore*: optional list of ignore matcher DSL strings.
/// *dry_run*: if true, print changes without writing them.
/// *confirm_yes*: if true, skip interactive confirmation prompts.
#[pyfunction]
#[pyo3(signature = (config, matcher, actions, ignore=None, dry_run=None, confirm_yes=None))]
fn execute_metadata_rule(
    config: &PyConfig,
    matcher: &str,
    actions: Vec<String>,
    ignore: Option<Vec<String>>,
    dry_run: Option<bool>,
    confirm_yes: Option<bool>,
) -> PyResult<()> {
    let action_refs: Vec<&str> = actions.iter().map(|s| s.as_str()).collect();
    let ignore_strs = ignore.unwrap_or_default();
    let ignore_refs: Vec<&str> = ignore_strs.iter().map(|s| s.as_str()).collect();
    let ignore_opt: Option<&[&str]> = if ignore_refs.is_empty() {
        None
    } else {
        Some(&ignore_refs)
    };
    let rule =
        rose_core::rule_parser::Rule::parse(matcher, &action_refs, ignore_opt).map_err(to_pyerr)?;
    rose_core::rules::execute_metadata_rule(
        &config.inner,
        &rule,
        dry_run.unwrap_or(false),
        confirm_yes.unwrap_or(true),
    )
    .map_err(to_pyerr)
}

// ===========================================================================
// Module definition
// ===========================================================================

/// The `rose` Python module — entry point for `import rose`.
#[pymodule]
fn rose(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Types
    m.add_class::<PyConfig>()?;
    m.add_class::<PyRelease>()?;
    m.add_class::<PyTrack>()?;
    m.add_class::<PyCollage>()?;
    m.add_class::<PyPlaylist>()?;
    m.add_class::<PyAudioTags>()?;

    // Cache / read functions
    m.add_function(wrap_pyfunction!(update_cache, m)?)?;
    m.add_function(wrap_pyfunction!(list_releases, m)?)?;
    m.add_function(wrap_pyfunction!(list_tracks, m)?)?;
    m.add_function(wrap_pyfunction!(list_collages, m)?)?;
    m.add_function(wrap_pyfunction!(list_playlists, m)?)?;
    m.add_function(wrap_pyfunction!(list_artists, m)?)?;
    m.add_function(wrap_pyfunction!(list_genres, m)?)?;
    m.add_function(wrap_pyfunction!(list_labels, m)?)?;
    m.add_function(wrap_pyfunction!(list_descriptors, m)?)?;
    m.add_function(wrap_pyfunction!(get_release, m)?)?;
    m.add_function(wrap_pyfunction!(get_track, m)?)?;
    m.add_function(wrap_pyfunction!(get_collage, m)?)?;
    m.add_function(wrap_pyfunction!(get_playlist, m)?)?;

    // Release write operations
    m.add_function(wrap_pyfunction!(toggle_release_new, m)?)?;
    m.add_function(wrap_pyfunction!(toggle_release_favorite, m)?)?;
    m.add_function(wrap_pyfunction!(set_release_rating, m)?)?;
    m.add_function(wrap_pyfunction!(delete_release, m)?)?;
    m.add_function(wrap_pyfunction!(set_release_cover_art, m)?)?;
    m.add_function(wrap_pyfunction!(delete_release_cover_art, m)?)?;
    m.add_function(wrap_pyfunction!(create_single_release, m)?)?;

    // Collage write operations
    m.add_function(wrap_pyfunction!(create_collage, m)?)?;
    m.add_function(wrap_pyfunction!(rename_collage, m)?)?;
    m.add_function(wrap_pyfunction!(delete_collage, m)?)?;
    m.add_function(wrap_pyfunction!(add_release_to_collage, m)?)?;
    m.add_function(wrap_pyfunction!(remove_release_from_collage, m)?)?;

    // Playlist write operations
    m.add_function(wrap_pyfunction!(create_playlist, m)?)?;
    m.add_function(wrap_pyfunction!(rename_playlist, m)?)?;
    m.add_function(wrap_pyfunction!(delete_playlist, m)?)?;
    m.add_function(wrap_pyfunction!(add_track_to_playlist, m)?)?;
    m.add_function(wrap_pyfunction!(remove_track_from_playlist, m)?)?;
    m.add_function(wrap_pyfunction!(set_playlist_cover_art, m)?)?;
    m.add_function(wrap_pyfunction!(delete_playlist_cover_art, m)?)?;

    // Rule execution
    m.add_function(wrap_pyfunction!(execute_stored_metadata_rules, m)?)?;
    m.add_function(wrap_pyfunction!(execute_metadata_rule, m)?)?;

    Ok(())
}
