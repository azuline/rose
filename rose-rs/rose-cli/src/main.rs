use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};
use tracing::{debug, info};

use rose_core::audiotags::AudioTags;
use rose_core::cache::{self, STORED_DATA_FILE_REGEX};
use rose_core::config::Config;
use rose_core::releases;
use rose_core::rule_parser::{Action, Matcher, Rule};
use rose_core::templates::{
    evaluate_release_template, evaluate_track_template, get_sample_music, PathTemplate,
};

mod dump;
mod watcher;

// ---------------------------------------------------------------------------
// Top-level CLI
// ---------------------------------------------------------------------------

/// A music manager with a virtual filesystem.
#[derive(Parser, Debug)]
#[command(name = "rose", version = rose_core::VERSION, about)]
struct Cli {
    /// Emit verbose (DEBUG-level) logging.
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Override the configuration file path.
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

// ---------------------------------------------------------------------------
// Top-level commands
// ---------------------------------------------------------------------------

#[derive(Subcommand, Debug)]
enum Commands {
    /// Print version.
    Version,

    /// Utilities for configuring Rosé.
    #[command(subcommand)]
    Config(ConfigCommands),

    /// Manage the read cache.
    #[command(subcommand)]
    Cache(CacheCommands),

    /// Manage releases.
    #[command(subcommand)]
    Releases(ReleasesCommands),

    /// Manage tracks.
    #[command(subcommand)]
    Tracks(TracksCommands),

    /// Manage collages.
    #[command(subcommand)]
    Collages(CollagesCommands),

    /// Manage playlists.
    #[command(subcommand)]
    Playlists(PlaylistsCommands),

    /// Manage artists.
    #[command(subcommand)]
    Artists(ArtistsCommands),

    /// Manage genres.
    #[command(subcommand)]
    Genres(GenresCommands),

    /// Manage labels.
    #[command(subcommand)]
    Labels(LabelsCommands),

    /// Manage descriptors.
    #[command(subcommand)]
    Descriptors(DescriptorsCommands),

    /// Run metadata update rules on the entire library.
    #[command(subcommand)]
    Rules(RulesCommands),

    /// Manage the virtual filesystem.
    #[command(subcommand)]
    Fs(FsCommands),
}

// ---------------------------------------------------------------------------
// config subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand, Debug)]
enum ConfigCommands {
    /// Generate a shell completion script.
    GenerateCompletion {
        /// Shell to generate completions for.
        shell: Shell,
    },

    /// Preview the configured path templates with sample data.
    PreviewTemplates,
}

// ---------------------------------------------------------------------------
// cache subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand, Debug)]
enum CacheCommands {
    /// Synchronize the read cache with new changes in the source directory.
    Update {
        /// Force re-read all data from disk, even for unchanged files.
        #[arg(short, long)]
        force: bool,
    },

    /// Start a watchdog to auto-update the cache when the source directory changes.
    Watch {
        /// Run the filesystem watcher in the foreground (default: daemon).
        #[arg(short, long)]
        foreground: bool,
    },

    /// Stop the running watchdog.
    Unwatch,
}

// ---------------------------------------------------------------------------
// releases subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand, Debug)]
enum ReleasesCommands {
    /// Print a single release (in JSON). Accepts a release's UUID/path.
    Print {
        /// Release UUID or path.
        release: String,
    },

    /// Print all releases (in JSON). Accepts an optional matcher to filter.
    PrintAll {
        /// Optional rules matcher.
        matcher: Option<String>,
    },

    /// Edit a release's metadata in $EDITOR. Accepts a release's UUID/path.
    Edit {
        /// Release UUID or path.
        release: String,

        /// Resume a failed release edit.
        #[arg(short, long)]
        resume: Option<PathBuf>,
    },

    /// Toggle a release's "new"-ness. Accepts a release's UUID/path.
    ToggleNew {
        /// Release UUID or path.
        release: String,
    },

    /// Toggle a release's "favorite" status. Accepts a release's UUID/path.
    ToggleFavorite {
        /// Release UUID or path.
        release: String,
    },

    /// Set a release's rating (1-100) or clear it. Accepts a release's UUID/path.
    SetRating {
        /// Release UUID or path.
        release: String,

        /// Rating value (1-100).
        rating: Option<u8>,

        /// Clear the rating (set to unrated).
        #[arg(long)]
        clear: bool,
    },

    /// Delete a release from the library. Accepts a release's UUID/path.
    Delete {
        /// Release UUID or path.
        release: String,
    },

    /// Set/replace the cover art of a release. Accepts a release's UUID/path.
    SetCover {
        /// Release UUID or path.
        release: String,

        /// Path to the cover art image.
        cover: PathBuf,
    },

    /// Delete the cover art of a release.
    DeleteCover {
        /// Release UUID or path.
        release: String,
    },

    /// Run rule engine actions on all tracks in a release.
    RunRule {
        /// Release UUID or path.
        release: String,

        /// Actions to run.
        actions: Vec<String>,

        /// Display intended changes without applying them.
        #[arg(short, long)]
        dry_run: bool,

        /// Bypass confirmation prompts.
        #[arg(short, long)]
        yes: bool,
    },

    /// Create a single release for the given track, and copy the track into it.
    CreateSingle {
        /// Path to the track file.
        track_path: PathBuf,

        /// Set the single as a loose track.
        #[arg(short, long)]
        loose_track: bool,
    },
}

// ---------------------------------------------------------------------------
// tracks subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand, Debug)]
enum TracksCommands {
    /// Print a single track (in JSON). Accepts a track's UUID/path.
    Print {
        /// Track UUID or path.
        track: String,
    },

    /// Print all tracks (in JSON). Accepts an optional matcher to filter.
    PrintAll {
        /// Optional rules matcher.
        matcher: Option<String>,
    },

    /// Run rule engine actions on a single track.
    RunRule {
        /// Track UUID or path.
        track: String,

        /// Actions to run.
        actions: Vec<String>,

        /// Display intended changes without applying them.
        #[arg(short, long)]
        dry_run: bool,

        /// Bypass confirmation prompts.
        #[arg(short, long)]
        yes: bool,
    },
}

// ---------------------------------------------------------------------------
// collages subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand, Debug)]
enum CollagesCommands {
    /// Create a new collage.
    Create {
        /// Collage name.
        name: String,
    },

    /// Rename a collage.
    Rename {
        /// Current collage name.
        old_name: String,
        /// New collage name.
        new_name: String,
    },

    /// Delete a collage.
    Delete {
        /// Collage name.
        collage: String,
    },

    /// Add a release to a collage.
    AddRelease {
        /// Collage name.
        collage: String,
        /// Release UUID or path.
        release: String,
    },

    /// Remove a release from a collage.
    RemoveRelease {
        /// Collage name.
        collage: String,
        /// Release UUID or path.
        release: String,
    },

    /// Edit (reorder/remove releases from) a collage in $EDITOR.
    Edit {
        /// Collage name.
        collage: String,
    },

    /// Print a collage (in JSON). Accepts a collage's name.
    Print {
        /// Collage name.
        collage: String,
    },

    /// Print all collages (in JSON).
    PrintAll,
}

// ---------------------------------------------------------------------------
// playlists subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand, Debug)]
enum PlaylistsCommands {
    /// Create a new playlist.
    Create {
        /// Playlist name.
        name: String,
    },

    /// Rename a playlist.
    Rename {
        /// Current playlist name.
        old_name: String,
        /// New playlist name.
        new_name: String,
    },

    /// Delete a playlist.
    Delete {
        /// Playlist name.
        playlist: String,
    },

    /// Add a track to a playlist.
    AddTrack {
        /// Playlist name.
        playlist: String,
        /// Track UUID or path.
        track: String,
    },

    /// Remove a track from a playlist.
    RemoveTrack {
        /// Playlist name.
        playlist: String,
        /// Track UUID or path.
        track: String,
    },

    /// Edit a playlist in $EDITOR.
    Edit {
        /// Playlist name.
        playlist: String,
    },

    /// Print a playlist (in JSON). Accepts a playlist's name.
    Print {
        /// Playlist name.
        playlist: String,
    },

    /// Print all playlists (in JSON).
    PrintAll,

    /// Set the cover art of a playlist.
    SetCover {
        /// Playlist name.
        playlist: String,
        /// Path to the cover art image.
        cover: PathBuf,
    },

    /// Delete the cover art of a playlist.
    DeleteCover {
        /// Playlist name.
        playlist: String,
    },
}

// ---------------------------------------------------------------------------
// artists subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand, Debug)]
enum ArtistsCommands {
    /// Print an artist (in JSON). Accepts an artist's name.
    Print {
        /// Artist name.
        artist: String,
    },

    /// Print all artists (in JSON).
    PrintAll,
}

// ---------------------------------------------------------------------------
// genres subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand, Debug)]
enum GenresCommands {
    /// Print a genre (in JSON). Accepts a genre's name.
    Print {
        /// Genre name.
        genre: String,
    },

    /// Print all genres (in JSON).
    PrintAll,
}

// ---------------------------------------------------------------------------
// labels subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand, Debug)]
enum LabelsCommands {
    /// Print a label (in JSON). Accepts a label's name.
    Print {
        /// Label name.
        label: String,
    },

    /// Print all labels (in JSON).
    PrintAll,
}

// ---------------------------------------------------------------------------
// descriptors subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand, Debug)]
enum DescriptorsCommands {
    /// Print a descriptor (in JSON). Accepts a descriptor's name.
    Print {
        /// Descriptor name.
        descriptor: String,
    },

    /// Print all descriptors (in JSON).
    PrintAll,
}

// ---------------------------------------------------------------------------
// rules subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand, Debug)]
enum RulesCommands {
    /// Run an ad hoc rule.
    Run {
        /// Matcher expression.
        matcher: String,

        /// Actions to run.
        actions: Vec<String>,

        /// Display intended changes without applying them.
        #[arg(short, long)]
        dry_run: bool,

        /// Bypass confirmation prompts.
        #[arg(short, long)]
        yes: bool,

        /// Ignore tracks matching this matcher (repeatable).
        #[arg(short, long)]
        ignore: Vec<String>,
    },

    /// Run the rules stored in the config.
    RunStored {
        /// Display intended changes without applying them.
        #[arg(short, long)]
        dry_run: bool,

        /// Bypass confirmation prompts.
        #[arg(short, long)]
        yes: bool,
    },
}

// ---------------------------------------------------------------------------
// fs subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand, Debug)]
enum FsCommands {
    /// Mount the virtual filesystem.
    Mount {
        /// Run the FUSE controller in the foreground (default: daemon).
        #[arg(short, long)]
        foreground: bool,
    },

    /// Unmount the virtual filesystem.
    Unmount,
}

// ---------------------------------------------------------------------------
// Lazy config loading
// ---------------------------------------------------------------------------

/// Load the Rose configuration on demand.
///
/// Subcommands that need config call this; `version` does not.
fn load_config(config_path: &Option<PathBuf>) -> Result<Config> {
    Ok(Config::parse(config_path.as_deref())?)
}

// ---------------------------------------------------------------------------
// Argument resolution helpers
// ---------------------------------------------------------------------------

fn valid_uuid(s: &str) -> bool {
    // UUIDs are 36 chars: 8-4-4-4-12, all hex digits and dashes
    if s.len() != 36 {
        return false;
    }
    s.chars().enumerate().all(|(i, c)| {
        if i == 8 || i == 13 || i == 18 || i == 23 {
            c == '-'
        } else {
            c.is_ascii_hexdigit()
        }
    })
}

fn parse_release_argument(r: &str) -> Result<String> {
    if valid_uuid(r) {
        debug!("Treating release argument {r} as UUID");
        return Ok(r.to_string());
    }
    // Treat as path, look for .rose.{uuid}.toml
    let p = Path::new(r);
    if let Ok(resolved) = p.canonicalize() {
        if let Ok(entries) = std::fs::read_dir(&resolved) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if let Some(m) = STORED_DATA_FILE_REGEX.captures(&name_str) {
                    let uuid = m.get(1).unwrap().as_str().to_string();
                    debug!("Parsed release ID {uuid} from release argument {r}");
                    return Ok(uuid);
                }
            }
        }
    }
    bail!(
        "{r} is not a valid release argument.\n\n\
         Release arguments must be one of:\n\n  \
         1. The release UUID\n  \
         2. The path of the source directory of a release\n  \
         3. The path of the release in the virtual filesystem (from any view)\n\n\
         {r} is not recognized as any of the above."
    )
}

fn parse_track_argument(t: &str) -> Result<String> {
    if valid_uuid(t) {
        debug!("Treating track argument {t} as UUID");
        return Ok(t.to_string());
    }
    // Treat as path, read audio tags
    let p = Path::new(t);
    if p.exists() {
        if let Ok(tags) = AudioTags::from_file(p) {
            if let Some(id) = tags.id {
                return Ok(id);
            }
        }
    }
    bail!(
        "{t} is not a valid track argument.\n\n\
         Track arguments must be one of:\n\n  \
         1. The track UUID\n  \
         2. The path of the track in the source directory\n  \
         3. The path of the track in the virtual filesystem (from any view)\n\n\
         {t} is not recognized as any of the above."
    )
}

// ---------------------------------------------------------------------------
// Daemonize helper
// ---------------------------------------------------------------------------

#[cfg(unix)]
fn daemonize(pid_path: Option<&Path>) -> Result<()> {
    use nix::sys::signal::kill;
    use nix::unistd::{fork, setsid, ForkResult};
    use std::io::Write;

    if let Some(pp) = pid_path {
        if pp.exists() {
            // Try to read existing PID
            match std::fs::read_to_string(pp) {
                Ok(content) => match content.trim().parse::<i32>() {
                    Ok(existing_pid) => {
                        let pid = nix::unistd::Pid::from_raw(existing_pid);
                        match kill(pid, None) {
                            Ok(_) => {
                                bail!("Daemon is already running in process {existing_pid}");
                            }
                            Err(_) => {
                                debug!("Ignoring pid file with a pid that isn't running: {existing_pid}");
                                let _ = std::fs::remove_file(pp);
                            }
                        }
                    }
                    Err(_) => {
                        debug!("Ignoring improperly formatted pid file at {}", pp.display());
                    }
                },
                Err(_) => {
                    debug!("Ignoring unreadable pid file at {}", pp.display());
                }
            }
        }
    }

    // Fork
    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            // Child: detach
            setsid().map_err(|e| anyhow::anyhow!("setsid failed: {e}"))?;
            Ok(())
        }
        Ok(ForkResult::Parent { child }) => {
            // Parent: write PID file and exit
            if let Some(pp) = pid_path {
                let mut f = std::fs::File::create(pp)?;
                write!(f, "{}", child)?;
            }
            std::process::exit(0);
        }
        Err(e) => bail!("fork failed: {e}"),
    }
}

// ---------------------------------------------------------------------------
// Template preview
// ---------------------------------------------------------------------------

fn preview_release_template(
    label: &str,
    template: &PathTemplate,
    samples: &[(rose_core::templates::Release, rose_core::templates::Track); 3],
) {
    eprintln!("\x1b[2;4m{label}:\x1b[0m");
    for (i, (release, _track)) in samples.iter().enumerate() {
        let rendered =
            evaluate_release_template(template, release, None, Some(&(i + 1).to_string()));
        eprintln!("\x1b[2m  Sample {}: \x1b[0m{rendered}", i + 1);
    }
}

fn preview_track_template(
    label: &str,
    template: &PathTemplate,
    samples: &[(rose_core::templates::Release, rose_core::templates::Track); 3],
) {
    eprintln!("\x1b[2;4m{label}:\x1b[0m");
    for (i, (_release, track)) in samples.iter().enumerate() {
        let rendered = evaluate_track_template(template, track, None, Some(&(i + 1).to_string()));
        eprintln!("\x1b[2m  Sample {}: \x1b[0m{rendered}", i + 1);
    }
}

fn preview_path_templates(c: &Config) {
    let samples = get_sample_music(&c.music_source_dir);

    preview_release_template(
        "Source Directory - Release",
        &c.path_templates.source.release,
        &samples,
    );
    preview_track_template(
        "Source Directory - Track",
        &c.path_templates.source.track,
        &samples,
    );
    eprintln!();
    preview_release_template(
        "1. Releases - Release",
        &c.path_templates.releases.release,
        &samples,
    );
    preview_track_template(
        "1. Releases - Track",
        &c.path_templates.releases.track,
        &samples,
    );
    eprintln!();
    preview_release_template(
        "1. Releases (New) - Release",
        &c.path_templates.releases_new.release,
        &samples,
    );
    preview_track_template(
        "1. Releases (New) - Track",
        &c.path_templates.releases_new.track,
        &samples,
    );
    eprintln!();
    preview_release_template(
        "1. Releases (Added On) - Release",
        &c.path_templates.releases_added_on.release,
        &samples,
    );
    preview_track_template(
        "1. Releases (Added On) - Track",
        &c.path_templates.releases_added_on.track,
        &samples,
    );
    eprintln!();
    preview_release_template(
        "1. Releases (Released On) - Release",
        &c.path_templates.releases_released_on.release,
        &samples,
    );
    preview_track_template(
        "1. Releases (Released On) - Track",
        &c.path_templates.releases_released_on.track,
        &samples,
    );
    eprintln!();
    preview_release_template(
        "2. Artists - Release",
        &c.path_templates.artists.release,
        &samples,
    );
    preview_track_template(
        "2. Artists - Track",
        &c.path_templates.artists.track,
        &samples,
    );
    eprintln!();
    preview_release_template(
        "3. Genres - Release",
        &c.path_templates.genres.release,
        &samples,
    );
    preview_track_template(
        "3. Genres - Track",
        &c.path_templates.genres.track,
        &samples,
    );
    eprintln!();
    preview_release_template(
        "4. Descriptors - Release",
        &c.path_templates.descriptors.release,
        &samples,
    );
    preview_track_template(
        "4. Descriptors - Track",
        &c.path_templates.descriptors.track,
        &samples,
    );
    eprintln!();
    preview_release_template(
        "5. Labels - Release",
        &c.path_templates.labels.release,
        &samples,
    );
    preview_track_template(
        "5. Labels - Track",
        &c.path_templates.labels.track,
        &samples,
    );
    eprintln!();
    preview_release_template(
        "7. Collages - Release",
        &c.path_templates.collages.release,
        &samples,
    );
    preview_track_template(
        "7. Collages - Track",
        &c.path_templates.collages.track,
        &samples,
    );
    eprintln!();
    preview_track_template(
        "8. Playlists - Track",
        &c.path_templates.playlists,
        &samples,
    );
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing: DEBUG if --verbose, INFO otherwise.
    let filter = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt().with_env_filter(filter).init();

    debug!("parsed CLI args: {:?}", cli);

    match cli.command {
        // -- version (no config needed) ------------------------------------
        Commands::Version => {
            println!("{}", rose_core::VERSION);
        }

        // -- config --------------------------------------------------------
        Commands::Config(cmd) => match cmd {
            ConfigCommands::GenerateCompletion { shell } => {
                let mut cmd = Cli::command();
                generate(shell, &mut cmd, "rose", &mut std::io::stdout());
            }
            ConfigCommands::PreviewTemplates => {
                let config = load_config(&cli.config)?;
                preview_path_templates(&config);
            }
        },

        // -- cache ---------------------------------------------------------
        Commands::Cache(cmd) => {
            let config = load_config(&cli.config)?;
            cache::maybe_invalidate_cache_database(&config)?;
            match cmd {
                CacheCommands::Update { force } => {
                    cache::update_cache(&config, force)?;
                }
                CacheCommands::Watch { foreground } => {
                    if !foreground {
                        #[cfg(unix)]
                        daemonize(Some(&config.watchdog_pid_path()))?;
                    }
                    watcher::start_watcher(&config)?;
                }
                CacheCommands::Unwatch => {
                    watcher::stop_watcher(&config)?;
                }
            }
        }

        // -- releases ------------------------------------------------------
        Commands::Releases(cmd) => {
            let config = load_config(&cli.config)?;
            match cmd {
                ReleasesCommands::Print { release } => {
                    let id = parse_release_argument(&release)?;
                    println!("{}", dump::dump_release(&config, &id)?);
                }
                ReleasesCommands::PrintAll { matcher } => {
                    let parsed_matcher = matcher.as_deref().map(Matcher::parse).transpose()?;
                    println!(
                        "{}",
                        dump::dump_all_releases(&config, parsed_matcher.as_ref())?
                    );
                }
                ReleasesCommands::Edit { release, resume } => {
                    let id = parse_release_argument(&release)?;
                    releases::edit_release(&config, &id, resume.as_deref())?;
                }
                ReleasesCommands::ToggleNew { release } => {
                    let id = parse_release_argument(&release)?;
                    releases::toggle_release_new(&config, &id)?;
                }
                ReleasesCommands::ToggleFavorite { release } => {
                    let id = parse_release_argument(&release)?;
                    releases::toggle_release_favorite(&config, &id)?;
                }
                ReleasesCommands::SetRating {
                    release,
                    rating,
                    clear,
                } => {
                    let id = parse_release_argument(&release)?;
                    if clear {
                        releases::set_release_rating(&config, &id, None)?;
                    } else if let Some(r) = rating {
                        releases::set_release_rating(&config, &id, Some(r))?;
                    } else {
                        bail!("Must provide a rating value (1-100) or use --clear.");
                    }
                }
                ReleasesCommands::Delete { release } => {
                    let id = parse_release_argument(&release)?;
                    releases::delete_release(&config, &id)?;
                }
                ReleasesCommands::SetCover { release, cover } => {
                    let id = parse_release_argument(&release)?;
                    releases::set_release_cover_art(&config, &id, &cover)?;
                }
                ReleasesCommands::DeleteCover { release } => {
                    let id = parse_release_argument(&release)?;
                    releases::delete_release_cover_art(&config, &id)?;
                }
                ReleasesCommands::RunRule {
                    release,
                    actions,
                    dry_run,
                    yes,
                } => {
                    let id = parse_release_argument(&release)?;
                    let parsed_actions: Vec<Action> = actions
                        .iter()
                        .enumerate()
                        .map(|(i, a)| Action::parse(a, Some(i + 1), None))
                        .collect::<std::result::Result<Vec<_>, _>>()?;
                    releases::run_actions_on_release(&config, &id, &parsed_actions, dry_run, !yes)?;
                }
                ReleasesCommands::CreateSingle {
                    track_path,
                    loose_track,
                } => {
                    let releasetype = if loose_track { "loosetrack" } else { "single" };
                    releases::create_single_release(&config, &track_path, releasetype)?;
                }
            }
        }

        // -- tracks --------------------------------------------------------
        Commands::Tracks(cmd) => {
            let config = load_config(&cli.config)?;
            match cmd {
                TracksCommands::Print { track } => {
                    let id = parse_track_argument(&track)?;
                    println!("{}", dump::dump_track(&config, &id)?);
                }
                TracksCommands::PrintAll { matcher } => {
                    let parsed_matcher = matcher.as_deref().map(Matcher::parse).transpose()?;
                    println!(
                        "{}",
                        dump::dump_all_tracks(&config, parsed_matcher.as_ref())?
                    );
                }
                TracksCommands::RunRule {
                    track,
                    actions,
                    dry_run,
                    yes,
                } => {
                    let id = parse_track_argument(&track)?;
                    let parsed_actions: Vec<Action> = actions
                        .iter()
                        .enumerate()
                        .map(|(i, a)| Action::parse(a, Some(i + 1), None))
                        .collect::<std::result::Result<Vec<_>, _>>()?;
                    rose_core::tracks::run_actions_on_track(
                        &config,
                        &id,
                        &parsed_actions,
                        dry_run,
                        !yes,
                    )?;
                }
            }
        }

        // -- collages ------------------------------------------------------
        Commands::Collages(cmd) => {
            let config = load_config(&cli.config)?;
            match cmd {
                CollagesCommands::Create { name } => {
                    rose_core::collages::create_collage(&config, &name)?;
                }
                CollagesCommands::Rename { old_name, new_name } => {
                    rose_core::collages::rename_collage(&config, &old_name, &new_name)?;
                }
                CollagesCommands::Delete { collage } => {
                    rose_core::collages::delete_collage(&config, &collage)?;
                }
                CollagesCommands::AddRelease { collage, release } => {
                    let id = parse_release_argument(&release)?;
                    rose_core::collages::add_release_to_collage(&config, &collage, &id)?;
                }
                CollagesCommands::RemoveRelease { collage, release } => {
                    let id = parse_release_argument(&release)?;
                    rose_core::collages::remove_release_from_collage(&config, &collage, &id)?;
                }
                CollagesCommands::Edit { collage } => {
                    rose_core::collages::edit_collage_in_editor(&config, &collage)?;
                }
                CollagesCommands::Print { collage } => {
                    println!("{}", dump::dump_collage(&config, &collage)?);
                }
                CollagesCommands::PrintAll => {
                    println!("{}", dump::dump_all_collages(&config)?);
                }
            }
        }

        // -- playlists -----------------------------------------------------
        Commands::Playlists(cmd) => {
            let config = load_config(&cli.config)?;
            match cmd {
                PlaylistsCommands::Create { name } => {
                    rose_core::playlists::create_playlist(&config, &name)?;
                }
                PlaylistsCommands::Rename { old_name, new_name } => {
                    rose_core::playlists::rename_playlist(&config, &old_name, &new_name)?;
                }
                PlaylistsCommands::Delete { playlist } => {
                    rose_core::playlists::delete_playlist(&config, &playlist)?;
                }
                PlaylistsCommands::AddTrack { playlist, track } => {
                    let id = parse_track_argument(&track)?;
                    rose_core::playlists::add_track_to_playlist(&config, &playlist, &id)?;
                }
                PlaylistsCommands::RemoveTrack { playlist, track } => {
                    let id = parse_track_argument(&track)?;
                    rose_core::playlists::remove_track_from_playlist(&config, &playlist, &id)?;
                }
                PlaylistsCommands::Edit { playlist } => {
                    rose_core::playlists::edit_playlist_in_editor(&config, &playlist)?;
                }
                PlaylistsCommands::Print { playlist } => {
                    println!("{}", dump::dump_playlist(&config, &playlist)?);
                }
                PlaylistsCommands::PrintAll => {
                    println!("{}", dump::dump_all_playlists(&config)?);
                }
                PlaylistsCommands::SetCover { playlist, cover } => {
                    rose_core::playlists::set_playlist_cover_art(&config, &playlist, &cover)?;
                }
                PlaylistsCommands::DeleteCover { playlist } => {
                    rose_core::playlists::delete_playlist_cover_art(&config, &playlist)?;
                }
            }
        }

        // -- artists -------------------------------------------------------
        Commands::Artists(cmd) => {
            let config = load_config(&cli.config)?;
            match cmd {
                ArtistsCommands::Print { artist } => {
                    println!("{}", dump::dump_artist(&config, &artist)?);
                }
                ArtistsCommands::PrintAll => {
                    println!("{}", dump::dump_all_artists(&config)?);
                }
            }
        }

        // -- genres --------------------------------------------------------
        Commands::Genres(cmd) => {
            let config = load_config(&cli.config)?;
            match cmd {
                GenresCommands::Print { genre } => {
                    println!("{}", dump::dump_genre(&config, &genre)?);
                }
                GenresCommands::PrintAll => {
                    println!("{}", dump::dump_all_genres(&config)?);
                }
            }
        }

        // -- labels --------------------------------------------------------
        Commands::Labels(cmd) => {
            let config = load_config(&cli.config)?;
            match cmd {
                LabelsCommands::Print { label } => {
                    println!("{}", dump::dump_label(&config, &label)?);
                }
                LabelsCommands::PrintAll => {
                    println!("{}", dump::dump_all_labels(&config)?);
                }
            }
        }

        // -- descriptors ---------------------------------------------------
        Commands::Descriptors(cmd) => {
            let config = load_config(&cli.config)?;
            match cmd {
                DescriptorsCommands::Print { descriptor } => {
                    println!("{}", dump::dump_descriptor(&config, &descriptor)?);
                }
                DescriptorsCommands::PrintAll => {
                    println!("{}", dump::dump_all_descriptors(&config)?);
                }
            }
        }

        // -- rules ---------------------------------------------------------
        Commands::Rules(cmd) => {
            let config = load_config(&cli.config)?;
            match cmd {
                RulesCommands::Run {
                    matcher,
                    actions,
                    dry_run,
                    yes,
                    ignore,
                } => {
                    if actions.is_empty() {
                        info!("No-Op: No actions passed");
                        return Ok(());
                    }
                    let action_strs: Vec<&str> = actions.iter().map(|s| s.as_str()).collect();
                    let ignore_strs: Vec<&str> = ignore.iter().map(|s| s.as_str()).collect();
                    let rule = Rule::parse(
                        &matcher,
                        &action_strs,
                        if ignore_strs.is_empty() {
                            None
                        } else {
                            Some(&ignore_strs)
                        },
                    )?;
                    rose_core::rules::execute_metadata_rule(&config, &rule, dry_run, !yes)?;
                }
                RulesCommands::RunStored { dry_run, yes } => {
                    rose_core::rules::execute_stored_metadata_rules(&config, dry_run, !yes)?;
                }
            }
        }

        // -- fs ------------------------------------------------------------
        Commands::Fs(cmd) => {
            let config = load_config(&cli.config)?;
            match cmd {
                FsCommands::Mount { foreground } => {
                    config.validate_path_templates_expensive()?;

                    if !foreground {
                        #[cfg(unix)]
                        daemonize(None)?;
                    }

                    // Spawn a background thread to update cache.
                    let config_clone = config.clone();
                    let cache_thread = std::thread::spawn(move || {
                        if let Err(e) = cache::update_cache(&config_clone, false) {
                            tracing::warn!("Background cache update failed: {e}");
                        }
                    });

                    // Launch rose-vfs binary as a subprocess with --foreground
                    // since we handle daemonization ourselves.
                    let mut vfs_cmd = std::process::Command::new("rose-vfs");
                    vfs_cmd.arg("--mount").arg(&config.vfs.mount_dir);
                    if cli.verbose {
                        vfs_cmd.arg("--debug");
                    }
                    if let Some(ref cp) = cli.config {
                        vfs_cmd.arg("--config").arg(cp);
                    }
                    let status = vfs_cmd.status()?;
                    if !status.success() {
                        bail!("rose-vfs exited with status: {status}");
                    }

                    let _ = cache_thread.join();
                }
                FsCommands::Unmount => {
                    let mount_dir = &config.vfs.mount_dir;
                    info!("Unmounting Rose VFS at {:?}", mount_dir);
                    let status = std::process::Command::new("umount")
                        .arg(mount_dir.to_string_lossy().as_ref())
                        .status()?;
                    if !status.success() {
                        tracing::warn!("umount exited with status: {status}");
                    }
                }
            }
        }
    }

    Ok(())
}
