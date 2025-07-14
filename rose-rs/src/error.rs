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
}

#[derive(Error, Debug)]
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
}

pub type Result<T> = std::result::Result<T, RoseError>;
