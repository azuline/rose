// Rose core library — foundation for the Rose music library manager.

pub mod audiotags;
pub mod cache;
pub mod collages;
pub mod common;
pub mod config;
pub mod genre_hierarchy;
pub mod playlists;
pub mod releases;
pub mod rule_parser;
pub mod rules;
pub mod templates;
pub mod tracks;
// mod vfs;               — virtual filesystem layer

/// Version string, kept in sync with `rose-py/rose/.version`.
pub const VERSION: &str = "0.5.0";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_non_empty() {
        assert!(!VERSION.is_empty());
    }
}
