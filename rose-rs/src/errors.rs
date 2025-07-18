use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RoseError {
    #[error("Rose error: {0}")]
    Generic(String),
    #[error(transparent)]
    Expected(#[from] RoseExpectedError),
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Regex error: {0}")]
    Regex(#[from] regex::Error),
    #[error("TOML deserialization error: {0}")]
    TomlDe(#[from] toml::de::Error),
    #[error("TOML serialization error: {0}")]
    TomlSer(#[from] toml::ser::Error),
    #[error("System time error: {0}")]
    SystemTime(#[from] std::time::SystemTimeError),
    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),
    #[error("Cache update error: {0}")]
    CacheUpdateError(String),
}

/// These errors are printed without traceback.
#[derive(Error, Debug, Clone)]
pub enum RoseExpectedError {
    #[error("{0}")]
    Generic(String),
    #[error("Genre does not exist: {name}")]
    GenreDoesNotExist { name: String },
    #[error("Label does not exist: {name}")]
    LabelDoesNotExist { name: String },
    #[error("Descriptor does not exist: {name}")]
    DescriptorDoesNotExist { name: String },
    #[error("Artist does not exist: {name}")]
    ArtistDoesNotExist { name: String },
    #[error("Invalid UUID: {uuid}")]
    InvalidUuid { uuid: String },
    #[error("File not found: {path}")]
    FileNotFound { path: PathBuf },
    #[error("Invalid file format: {format}")]
    InvalidFileFormat { format: String },
    #[error("Release does not exist: {id}")]
    ReleaseDoesNotExist { id: String },
    #[error("Track does not exist: {id}")]
    TrackDoesNotExist { id: String },
    #[error("Collage does not exist: {name}")]
    CollageDoesNotExist { name: String },
    #[error("Playlist does not exist: {name}")]
    PlaylistDoesNotExist { name: String },
    #[error("{0}")]
    InvalidRule(String),
}

pub type Result<T> = std::result::Result<T, RoseError>;
