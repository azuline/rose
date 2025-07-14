pub mod common;
pub mod error;

pub use common::{Artist, ArtistMapping};
pub use error::{RoseError, RoseExpectedError, Result};

#[cfg(test)]
mod common_test;