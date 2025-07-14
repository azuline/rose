#![allow(clippy::uninlined_format_args)]

pub mod audiotags;
pub mod cache;
pub mod cache_update;
pub mod collages;
pub mod common;
pub mod config;
pub mod datafiles;
pub mod error;
pub mod genre_hierarchy;
pub mod playlists;
pub mod releases;
pub mod rule_parser;
pub mod rules;
pub mod templates;
pub mod tracks;

pub use common::{Artist, ArtistMapping};
pub use config::Config;
pub use error::{Result, RoseError, RoseExpectedError};

#[cfg(test)]
mod audiotags_test;
#[cfg(test)]
mod collages_test;
#[cfg(test)]
mod common_test;
#[cfg(test)]
mod config_test;
#[cfg(test)]
mod datafiles_test;
#[cfg(test)]
mod genre_hierarchy_test;
#[cfg(test)]
mod playlists_test;
#[cfg(test)]
mod releases_test;
#[cfg(test)]
mod rule_parser_test;
#[cfg(test)]
mod rules_test;
#[cfg(test)]
mod templates_test;
#[cfg(test)]
mod tracks_test;
#[cfg(test)]
mod test_utils;
