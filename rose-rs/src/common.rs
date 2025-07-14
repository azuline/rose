use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use regex::Regex;
use unicode_normalization::UnicodeNormalization;
use sha2::{Sha256, Digest};

lazy_static::lazy_static! {
    static ref ILLEGAL_FS_CHARS_REGEX: Regex = Regex::new(r#"[:\?<>\\*\|"/]+"#).unwrap();
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
        let all = [
            &self.main[..],
            &self.guest[..],
            &self.remixer[..],
            &self.producer[..],
            &self.composer[..],
            &self.conductor[..],
            &self.djmixer[..],
        ].concat();
        uniq(all)
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
        ].into_iter()
    }
}

pub fn flatten<T>(xxs: Vec<Vec<T>>) -> Vec<T> {
    xxs.into_iter().flatten().collect()
}

pub fn uniq<T: Hash + Eq + Clone>(xs: Vec<T>) -> Vec<T> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    
    for x in xs {
        if seen.insert(x.clone()) {
            result.push(x);
        }
    }
    
    result
}

pub fn sanitize_dirname(name: &str, max_filename_bytes: usize, enforce_maxlen: bool) -> String {
    let mut name = ILLEGAL_FS_CHARS_REGEX.replace_all(name, "_").into_owned();
    
    if enforce_maxlen {
        let bytes = name.as_bytes();
        if bytes.len() > max_filename_bytes {
            name = String::from_utf8_lossy(&bytes[..max_filename_bytes])
                .trim()
                .to_string();
        }
    }
    
    name.nfd().collect()
}

pub fn sanitize_filename(name: &str, max_filename_bytes: usize, enforce_maxlen: bool) -> String {
    let mut name = ILLEGAL_FS_CHARS_REGEX.replace_all(name, "_").into_owned();
    
    if enforce_maxlen {
        let (stem, ext) = match name.rfind('.') {
            Some(pos) => {
                let (s, e) = name.split_at(pos);
                (s, e)
            }
            None => (name.as_str(), ""),
        };
        
        // Ignore extension if it's longer than 6 bytes
        let (stem, ext) = if ext.len() > 6 {
            (name.as_str(), "")
        } else {
            (stem, ext)
        };
        
        let stem_bytes = stem.as_bytes();
        let stem = if stem_bytes.len() > max_filename_bytes {
            String::from_utf8_lossy(&stem_bytes[..max_filename_bytes])
                .trim()
                .to_string()
        } else {
            stem.to_string()
        };
        
        name = format!("{stem}{ext}");
    }
    
    name.nfd().collect()
}

pub fn sha256_dataclass<T: std::fmt::Debug>(value: &T) -> String {
    let mut hasher = Sha256::new();
    let debug_str = format!("{value:?}");
    hasher.update(debug_str.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn initialize_logging(_logger_name: Option<&str>, output: &str) {
    use tracing_subscriber::{fmt, EnvFilter};
    
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));
    
    let builder = fmt().with_env_filter(filter);
    
    match output {
        "stderr" => {
            builder.init();
        }
        "file" => {
            // For file output, we'd need to set up file appender
            // For now, just use stderr
            builder.init();
        }
        _ => {
            builder.init();
        }
    }
}

pub const SUPPORTED_AUDIO_EXTENSIONS: &[&str] = &[
    ".mp3",
    ".m4a",
    ".ogg",
    ".opus",
    ".flac",
];

pub const SUPPORTED_IMAGE_EXTENSIONS: &[&str] = &[
    ".jpg",
    ".jpeg",
    ".png",
];

pub fn is_music_file(path: &str) -> bool {
    let path_lower = path.to_lowercase();
    SUPPORTED_AUDIO_EXTENSIONS.iter().any(|ext| path_lower.ends_with(ext))
}

pub fn is_image_file(path: &str) -> bool {
    let path_lower = path.to_lowercase();
    SUPPORTED_IMAGE_EXTENSIONS.iter().any(|ext| path_lower.ends_with(ext))
}
