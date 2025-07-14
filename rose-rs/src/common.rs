use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use unicode_normalization::UnicodeNormalization;

lazy_static::lazy_static! {
    static ref ILLEGAL_FS_CHARS_REGEX: Regex = Regex::new(r#"[:\?<>\\*\|"/]+"#).unwrap();
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artist {
    pub name: String,
    pub alias: bool,
}

impl Artist {
    pub fn new(name: String) -> Self {
        Self { name, alias: false }
    }

    pub fn with_alias(name: String, alias: bool) -> Self {
        Self { name, alias }
    }
}

impl Hash for Artist {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.alias.hash(state);
    }
}

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
    pub fn new() -> Self {
        Self::default()
    }

    pub fn all(&self) -> Vec<Artist> {
        uniq(
            [
                &self.main[..],
                &self.guest[..],
                &self.remixer[..],
                &self.producer[..],
                &self.composer[..],
                &self.conductor[..],
                &self.djmixer[..],
            ]
            .concat(),
        )
    }

    pub fn items(&self) -> impl Iterator<Item = (&'static str, &Vec<Artist>)> {
        [
            ("main", &self.main),
            ("guest", &self.guest),
            ("remixer", &self.remixer),
            ("producer", &self.producer),
            ("composer", &self.composer),
            ("conductor", &self.conductor),
            ("djmixer", &self.djmixer),
        ]
        .into_iter()
    }
}

pub fn flatten<T>(xxs: Vec<Vec<T>>) -> Vec<T> {
    xxs.into_iter().flatten().collect()
}

pub fn uniq<T: Hash + Eq + Clone>(xs: Vec<T>) -> Vec<T> {
    let mut seen = HashSet::new();
    xs.into_iter().filter(|x| seen.insert(x.clone())).collect()
}

fn sanitize(name: &str, max_bytes: usize, enforce_max: bool) -> String {
    let mut name = ILLEGAL_FS_CHARS_REGEX.replace_all(name, "_").into_owned();
    if enforce_max && name.len() > max_bytes {
        name.truncate(max_bytes);
        name = name.trim().to_string();
    }
    name.nfd().collect()
}

pub fn sanitize_dirname(name: &str, max_filename_bytes: usize, enforce_maxlen: bool) -> String {
    sanitize(name, max_filename_bytes, enforce_maxlen)
}

pub fn sanitize_filename(name: &str, max_filename_bytes: usize, enforce_maxlen: bool) -> String {
    let mut name = ILLEGAL_FS_CHARS_REGEX.replace_all(name, "_").into_owned();

    if enforce_maxlen {
        let (stem, ext) = name.rsplit_once('.').map_or((name.as_str(), ""), |(s, e)| {
            if e.len() > 6 {
                (name.as_str(), "")
            } else {
                (s, &name[s.len()..])
            }
        });

        let mut stem = stem.to_string();
        if stem.len() > max_filename_bytes {
            stem.truncate(max_filename_bytes);
            stem = stem.trim().to_string();
        }
        name = format!("{stem}{ext}");
    }

    name.nfd().collect()
}

pub fn sha256_dataclass<T: std::fmt::Debug>(value: &T) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{value:?}"));
    format!("{:x}", hasher.finalize())
}

pub fn initialize_logging(_logger_name: Option<&str>, output: &str) {
    use tracing_subscriber::{fmt, EnvFilter};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let builder = fmt().with_env_filter(filter);

    match output {
        "file" => builder.init(), // TODO: implement file logging
        _ => builder.init(),
    }
}

pub const SUPPORTED_AUDIO_EXTENSIONS: &[&str] = &[".mp3", ".m4a", ".ogg", ".opus", ".flac"];
pub const SUPPORTED_IMAGE_EXTENSIONS: &[&str] = &[".jpg", ".jpeg", ".png"];

pub fn is_music_file(path: &str) -> bool {
    let path_lower = path.to_lowercase();
    SUPPORTED_AUDIO_EXTENSIONS
        .iter()
        .any(|ext| path_lower.ends_with(ext))
}

pub fn is_image_file(path: &str) -> bool {
    let path_lower = path.to_lowercase();
    SUPPORTED_IMAGE_EXTENSIONS
        .iter()
        .any(|ext| path_lower.ends_with(ext))
}
