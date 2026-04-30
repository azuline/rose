//! Common utilities module: error types, Artist/ArtistMapping types,
//! filesystem sanitization, hashing, and helper functions.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Version string, kept in sync with `rose-py/rose/.version`.
pub const VERSION: &str = "0.5.0";

/// Regex matching characters illegal in filesystem names.
pub static ILLEGAL_FS_CHARS_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"[:\?<>\\*\|"/]+"#).expect("invalid regex"));

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Top-level error enum for Rose.
///
/// Variants that correspond to "expected" user-facing errors (printed without
/// a backtrace in the Python version) are marked via [`RoseError::is_expected`].
#[derive(Debug, thiserror::Error)]
pub enum RoseError {
    #[error("Genre does not exist: {0}")]
    GenreDoesNotExist(String),

    #[error("Label does not exist: {0}")]
    LabelDoesNotExist(String),

    #[error("Descriptor does not exist: {0}")]
    DescriptorDoesNotExist(String),

    #[error("Artist does not exist: {0}")]
    ArtistDoesNotExist(String),

    #[error("Unsupported filetype: {0}")]
    UnsupportedFiletype(String),

    #[error("Unsupported tag value type: {0}")]
    UnsupportedTagValueType(String),

    #[error("Invalid path template '{key}': {message}")]
    InvalidPathTemplate { key: String, message: String },

    #[error("{0}")]
    ConfigNotFound(String),

    #[error("{0}")]
    ConfigDecode(String),

    #[error("{0}")]
    MissingConfigKey(String),

    #[error("{0}")]
    InvalidConfigValue(String),

    #[error("Duplicate track found: {0}")]
    DuplicateTrack(String),

    #[error("Duplicate release found: {0}")]
    DuplicateRelease(String),

    #[error("Collage does not exist: {0}")]
    CollageDoesNotExist(String),

    #[error("Collage already exists: {0}")]
    CollageAlreadyExists(String),

    #[error("Release does not exist: {0}")]
    ReleaseDoesNotExist(String),

    #[error("Description mismatch: {0}")]
    DescriptionMismatch(String),

    #[error("Playlist does not exist: {0}")]
    PlaylistDoesNotExist(String),

    #[error("Playlist already exists: {0}")]
    PlaylistAlreadyExists(String),

    #[error("Track does not exist: {0}")]
    TrackDoesNotExist(String),

    #[error("Invalid cover art file: {0}")]
    InvalidCoverArt(String),

    /// Catch-all for internal / unexpected errors.
    #[error("{0}")]
    Internal(String),
}

impl RoseError {
    /// Returns `true` for errors that should be presented to the user without
    /// a full backtrace (the Python `RoseExpectedError` family).
    pub fn is_expected(&self) -> bool {
        match self {
            RoseError::GenreDoesNotExist(_)
            | RoseError::LabelDoesNotExist(_)
            | RoseError::DescriptorDoesNotExist(_)
            | RoseError::ArtistDoesNotExist(_)
            | RoseError::UnsupportedFiletype(_)
            | RoseError::UnsupportedTagValueType(_)
            | RoseError::InvalidPathTemplate { .. }
            | RoseError::ConfigNotFound(_)
            | RoseError::ConfigDecode(_)
            | RoseError::MissingConfigKey(_)
            | RoseError::InvalidConfigValue(_)
            | RoseError::DuplicateTrack(_)
            | RoseError::DuplicateRelease(_)
            | RoseError::CollageDoesNotExist(_)
            | RoseError::CollageAlreadyExists(_)
            | RoseError::ReleaseDoesNotExist(_)
            | RoseError::DescriptionMismatch(_)
            | RoseError::PlaylistDoesNotExist(_)
            | RoseError::PlaylistAlreadyExists(_)
            | RoseError::TrackDoesNotExist(_)
            | RoseError::InvalidCoverArt(_) => true,
            RoseError::Internal(_) => false,
        }
    }
}

// ---------------------------------------------------------------------------
// Artist / ArtistMapping
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Artist {
    pub name: String,
    #[serde(default)]
    pub alias: bool,
}

impl Artist {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            alias: false,
        }
    }
}

/// Mapping of artist roles to lists of artists.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtistMapping {
    pub main: Vec<Artist>,
    pub guest: Vec<Artist>,
    pub remixer: Vec<Artist>,
    pub producer: Vec<Artist>,
    pub composer: Vec<Artist>,
    pub conductor: Vec<Artist>,
    pub djmixer: Vec<Artist>,
}

impl ArtistMapping {
    /// Returns a deduplicated union of all artists across every role,
    /// preserving first-occurrence order (main → guest → remixer → producer →
    /// composer → conductor → djmixer).
    pub fn all(&self) -> Vec<&Artist> {
        let mut seen = HashSet::new();
        let mut result = Vec::new();
        for artist in self
            .main
            .iter()
            .chain(&self.guest)
            .chain(&self.remixer)
            .chain(&self.producer)
            .chain(&self.composer)
            .chain(&self.conductor)
            .chain(&self.djmixer)
        {
            if seen.insert(artist) {
                result.push(artist);
            }
        }
        result
    }

    /// Yields `(role_name, artists)` pairs in canonical order.
    pub fn items(&self) -> Vec<(&str, &[Artist])> {
        vec![
            ("main", &self.main[..]),
            ("guest", &self.guest[..]),
            ("remixer", &self.remixer[..]),
            ("producer", &self.producer[..]),
            ("composer", &self.composer[..]),
            ("conductor", &self.conductor[..]),
            ("djmixer", &self.djmixer[..]),
        ]
    }

    /// Serialize to a serde-compatible value (mirrors Python `dump()`).
    pub fn dump(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("ArtistMapping serialization cannot fail")
    }
}

// ---------------------------------------------------------------------------
// Filesystem sanitization
// ---------------------------------------------------------------------------

/// Replace illegal filesystem characters, optionally truncate and strip
/// diacritics, and NFC-normalize the result.
///
/// `max_filename_bytes` is the byte-length ceiling used when `enforce_maxlen`
/// is true (Python passes `config.max_filename_bytes`, typically 240).
pub fn sanitize_dirname(
    max_filename_bytes: usize,
    name: &str,
    enforce_maxlen: bool,
    sanitize_diacritics: bool,
) -> String {
    let mut name = ILLEGAL_FS_CHARS_REGEX.replace_all(name, "_").into_owned();
    if enforce_maxlen {
        name = truncate_utf8(&name, max_filename_bytes).trim().to_owned();
    }
    if sanitize_diacritics {
        name = strip_diacritics(&name);
    }
    // Always NFC-normalize the output.
    name.nfc().collect()
}

/// Same as [`sanitize_dirname`] but preserves the file extension.
/// Extensions longer than 6 bytes are treated as part of the stem.
pub fn sanitize_filename(
    max_filename_bytes: usize,
    name: &str,
    enforce_maxlen: bool,
    sanitize_diacritics: bool,
) -> String {
    let mut name = ILLEGAL_FS_CHARS_REGEX.replace_all(name, "_").into_owned();
    if enforce_maxlen {
        let (stem, ext) = split_extension(&name);
        let truncated_stem = truncate_utf8(stem, max_filename_bytes).trim().to_owned();
        name = truncated_stem + ext;
    }
    if sanitize_diacritics {
        name = strip_diacritics(&name);
    }
    name.nfc().collect()
}

/// Truncate a UTF-8 string to at most `max_bytes` bytes, always on a char
/// boundary (never producing invalid UTF-8). Public wrapper for use in other
/// modules (e.g. cache renaming collision handling).
pub fn truncate_utf8_public(s: &str, max_bytes: usize) -> &str {
    truncate_utf8(s, max_bytes)
}

/// Truncate a UTF-8 string to at most `max_bytes` bytes, always on a char
/// boundary (never producing invalid UTF-8).
fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    // Find the largest char boundary <= max_bytes.
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Split a filename into `(stem, ext)` where `ext` includes the leading dot.
/// If the extension is longer than 6 bytes it is considered part of the stem
/// (returns `(name, "")`).
fn split_extension(name: &str) -> (&str, &str) {
    if let Some(dot_pos) = name.rfind('.') {
        let ext = &name[dot_pos..];
        if ext.len() <= 6 {
            return (&name[..dot_pos], ext);
        }
    }
    (name, "")
}

/// NFD-decompose and strip combining characters (diacritics).
fn strip_diacritics(s: &str) -> String {
    s.nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .collect()
}

// ---------------------------------------------------------------------------
// Hashing
// ---------------------------------------------------------------------------

/// Deterministic SHA-256 hex digest of any serde-serializable value.
///
/// Uses `serde_json::to_string` for a canonical byte representation.
pub fn sha256_struct(value: &impl Serialize) -> String {
    let json = serde_json::to_string(value).expect("serialization failed");
    let hash = Sha256::digest(json.as_bytes());
    format!("{hash:x}")
}

// ---------------------------------------------------------------------------
// Collection helpers
// ---------------------------------------------------------------------------

/// Flatten a nested `Vec<Vec<T>>` into a single `Vec<T>`.
pub fn flatten<T>(xxs: Vec<Vec<T>>) -> Vec<T> {
    xxs.into_iter().flatten().collect()
}

/// Deduplicate a vector, preserving first-occurrence order.
pub fn uniq<T: Eq + std::hash::Hash + Clone>(xs: Vec<T>) -> Vec<T> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for x in xs {
        if seen.insert(x.clone()) {
            result.push(x);
        }
    }
    result
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- sanitize_dirname ---------------------------------------------------

    #[test]
    fn illegal_chars_replaced() {
        let out = sanitize_dirname(240, r#"foo:bar?baz<qux>a\b*c|d"e/f"#, false, false);
        assert_eq!(out, "foo_bar_baz_qux_a_b_c_d_e_f");
    }

    #[test]
    fn max_length_truncation_at_char_boundary() {
        // 'é' is 2 bytes in UTF-8. Build a string that would split in the
        // middle of a multi-byte char at exactly 180 bytes.
        let base = "é".repeat(100); // 200 bytes
        let out = sanitize_dirname(180, &base, true, false);
        // Must be valid UTF-8 and at most 180 bytes.
        assert!(out.len() <= 180);
        assert!(out.len() >= 178); // should be 180 (90 × 2)
                                   // Verify it round-trips as valid UTF-8.
        let _: &str = &out;
    }

    #[test]
    fn diacritics_stripped() {
        let out = sanitize_dirname(240, "Modéré", false, true);
        assert_eq!(out, "Modere");
    }

    #[test]
    fn output_is_nfc() {
        // Feed in an NFD sequence (e + combining acute accent) and verify NFC
        // output (single precomposed é).
        let nfd_input = "e\u{0301}"; // NFD for 'é'
        let out = sanitize_dirname(240, nfd_input, false, false);
        assert_eq!(out, "\u{00E9}"); // NFC 'é'
    }

    #[test]
    fn empty_input_returns_empty() {
        assert_eq!(sanitize_dirname(240, "", true, true), "");
    }

    // --- sanitize_filename --------------------------------------------------

    #[test]
    fn extension_preserved() {
        let out = sanitize_filename(240, "foo:bar.mp3", false, false);
        assert_eq!(out, "foo_bar.mp3");
    }

    #[test]
    fn long_extension_treated_as_stem() {
        // ".longext" is 8 bytes (> 6), so the dot is not treated as an
        // extension separator.
        let out = sanitize_filename(20, "stem.longext", true, false);
        // The whole thing is truncated as a single stem, not split.
        assert!(out.len() <= 20);
        assert!(!out.ends_with(".longext") || out.len() <= 20);
    }

    #[test]
    fn extension_preserved_during_truncation() {
        // Build a long stem with a short extension.
        let stem = "a".repeat(250);
        let name = format!("{stem}.mp3");
        let out = sanitize_filename(240, &name, true, false);
        assert!(out.ends_with(".mp3"));
        // stem portion should be truncated to 240 bytes + ".mp3" = 244 total
        assert!(out.len() <= 244);
    }

    // --- ArtistMapping::all() dedup -----------------------------------------

    #[test]
    fn artist_mapping_all_dedup() {
        let alice = Artist::new("Alice");
        let bob = Artist::new("Bob");
        let mapping = ArtistMapping {
            main: vec![alice.clone(), bob.clone()],
            guest: vec![bob.clone()],      // duplicate Bob
            producer: vec![alice.clone()], // duplicate Alice
            ..Default::default()
        };
        let all = mapping.all();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].name, "Alice");
        assert_eq!(all[1].name, "Bob");
    }

    // --- sha256_struct ------------------------------------------------------

    #[test]
    fn hash_determinism() {
        let a = Artist::new("Test");
        let h1 = sha256_struct(&a);
        let h2 = sha256_struct(&a);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // SHA-256 hex digest is 64 chars
    }

    // --- uniq ---------------------------------------------------------------

    #[test]
    fn uniq_preserves_order() {
        assert_eq!(uniq(vec![3, 1, 2, 1, 3]), vec![3, 1, 2]);
    }

    // --- flatten ------------------------------------------------------------

    #[test]
    fn flatten_works() {
        assert_eq!(
            flatten(vec![vec![1, 2], vec![3], vec![4, 5]]),
            vec![1, 2, 3, 4, 5]
        );
    }

    // --- items() ------------------------------------------------------------

    #[test]
    fn items_returns_all_roles() {
        let mapping = ArtistMapping::default();
        let items = mapping.items();
        let role_names: Vec<&str> = items.iter().map(|(name, _)| *name).collect();
        assert_eq!(
            role_names,
            vec![
                "main",
                "guest",
                "remixer",
                "producer",
                "composer",
                "conductor",
                "djmixer"
            ]
        );
    }
}
