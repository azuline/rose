pub mod common;
pub mod error;
pub mod genre_hierarchy;

pub use common::{Artist, ArtistMapping};
pub use error::{RoseError, RoseExpectedError, Result};

#[cfg(test)]
mod common_test;
#[cfg(test)]
mod genre_hierarchy_test;