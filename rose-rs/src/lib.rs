pub mod common;
pub mod config;
pub mod error;
pub mod genre_hierarchy;

pub use common::{Artist, ArtistMapping};
pub use config::Config;
pub use error::{Result, RoseError, RoseExpectedError};

#[cfg(test)]
mod common_test;
#[cfg(test)]
mod config_test;
#[cfg(test)]
mod genre_hierarchy_test;
