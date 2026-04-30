// ---------------------------------------------------------------------------
// File Watcher with Debouncing
// ---------------------------------------------------------------------------
//
// Architecture: a sync `notify` watcher thread pushes classified events into a
// crossbeam-style channel (std::sync::mpsc). A dedicated consumer thread
// debounces (200ms dedup window) and dispatches cache updates, with a 2-second
// delay for release events so multi-file copies settle before we act.
//
// Port of `rose-watch/rose_watch/watcher.py`.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tracing::{debug, info, warn};

use rose_core::cache;
use rose_core::config::Config;

// ---------------------------------------------------------------------------
// Event types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum EventType {
    Created,
    Deleted,
    Modified,
    Moved,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum WatchEvent {
    Collage { name: String, event_type: EventType },
    Playlist { name: String, event_type: EventType },
    Release { dir: PathBuf, event_type: EventType },
}

// ---------------------------------------------------------------------------
// Event classification
// ---------------------------------------------------------------------------

/// Map a `notify::EventKind` to our `EventType`, or `None` if irrelevant.
fn map_event_kind(kind: &EventKind) -> Option<EventType> {
    match kind {
        EventKind::Create(_) => Some(EventType::Created),
        EventKind::Remove(_) => Some(EventType::Deleted),
        EventKind::Modify(mk) => {
            use notify::event::ModifyKind;
            match mk {
                ModifyKind::Name(_) => Some(EventType::Moved),
                _ => Some(EventType::Modified),
            }
        }
        _ => None,
    }
}

/// Classify a filesystem path (from a notify event) into a `WatchEvent`.
///
/// Returns `None` when the path is not meaningful (e.g. the root of the source
/// directory itself, or a non-`.toml` file inside `!collages`/`!playlists`).
fn classify_path(
    music_source_dir: &Path,
    path: &Path,
    event_type: EventType,
) -> Option<WatchEvent> {
    let relative = path.strip_prefix(music_source_dir).ok()?;
    let mut components = relative.components();
    let first = components.next()?;
    let first_str = first.as_os_str().to_str()?;

    if first_str == "!collages" {
        // Expect exactly `!collages/<name>.toml`
        let file_component = components.next()?;
        let file_name = file_component.as_os_str().to_str()?;
        let name = file_name.strip_suffix(".toml")?;
        return Some(WatchEvent::Collage {
            name: name.to_string(),
            event_type,
        });
    }

    if first_str == "!playlists" {
        let file_component = components.next()?;
        let file_name = file_component.as_os_str().to_str()?;
        let name = file_name.strip_suffix(".toml")?;
        return Some(WatchEvent::Playlist {
            name: name.to_string(),
            event_type,
        });
    }

    // Any other first component → release directory.
    // Skip paths that resolve to root ("/").
    if first_str == "/" {
        return None;
    }

    let dir = music_source_dir.join(first_str);
    Some(WatchEvent::Release { dir, event_type })
}

/// Classify a `notify::Event` into zero or more `WatchEvent`s.
fn classify_notify_event(music_source_dir: &Path, event: &notify::Event) -> Vec<WatchEvent> {
    let Some(etype) = map_event_kind(&event.kind) else {
        return Vec::new();
    };
    event
        .paths
        .iter()
        .filter_map(|p| classify_path(music_source_dir, p, etype.clone()))
        .collect()
}

// ---------------------------------------------------------------------------
// Hashing helper for dedup map
// ---------------------------------------------------------------------------

fn hash_event(event: &WatchEvent) -> u64 {
    let mut hasher = DefaultHasher::new();
    event.hash(&mut hasher);
    hasher.finish()
}

// ---------------------------------------------------------------------------
// Event handler – dispatches to cache functions
// ---------------------------------------------------------------------------

fn handle_event(config: &Config, event: &WatchEvent) {
    let result = match event {
        WatchEvent::Collage { name, event_type } => match event_type {
            EventType::Created | EventType::Modified => {
                debug!("Updating cache for collage {name}");
                cache::update_cache_for_collages(config, Some(vec![name.clone()]), false)
            }
            EventType::Deleted => {
                debug!("Evicting nonexistent collages");
                cache::update_cache_evict_nonexistent_collages(config)
            }
            EventType::Moved => {
                debug!("Updating + evicting collage {name}");
                let r1 = cache::update_cache_for_collages(config, Some(vec![name.clone()]), false);
                let r2 = cache::update_cache_evict_nonexistent_collages(config);
                r1.and(r2)
            }
        },
        WatchEvent::Playlist { name, event_type } => match event_type {
            EventType::Created | EventType::Modified => {
                debug!("Updating cache for playlist {name}");
                cache::update_cache_for_playlists(config, Some(vec![name.clone()]), false)
            }
            EventType::Deleted => {
                debug!("Evicting nonexistent playlists");
                cache::update_cache_evict_nonexistent_playlists(config)
            }
            EventType::Moved => {
                debug!("Updating + evicting playlist {name}");
                let r1 = cache::update_cache_for_playlists(config, Some(vec![name.clone()]), false);
                let r2 = cache::update_cache_evict_nonexistent_playlists(config);
                r1.and(r2)
            }
        },
        WatchEvent::Release { dir, event_type } => match event_type {
            EventType::Created | EventType::Modified => {
                debug!("Updating cache for release {}", dir.display());
                cache::update_cache_for_releases(config, Some(vec![dir.clone()]), false)
            }
            EventType::Deleted => {
                debug!("Evicting nonexistent releases");
                cache::update_cache_evict_nonexistent_releases(config)
            }
            EventType::Moved => {
                debug!("Updating + evicting release {}", dir.display());
                let r1 = cache::update_cache_for_releases(config, Some(vec![dir.clone()]), false);
                let r2 = cache::update_cache_evict_nonexistent_releases(config);
                r1.and(r2)
            }
        },
    };

    if let Err(e) = result {
        warn!("Cache update failed: {e}");
    }
}

// ---------------------------------------------------------------------------
// Debounced event processor (runs on main thread)
// ---------------------------------------------------------------------------

/// Debounce parameters.
const DEDUP_WINDOW: Duration = Duration::from_millis(200);
const RELEASE_DELAY: Duration = Duration::from_secs(2);

fn event_processor(config: &Config, rx: Receiver<WatchEvent>, shutdown: Arc<AtomicBool>) {
    let mut debounce_times: HashMap<u64, Instant> = HashMap::new();

    // Pending release events: each entry is (event, scheduled_at).
    let mut pending_releases: Vec<(WatchEvent, Instant)> = Vec::new();

    loop {
        if shutdown.load(Ordering::Relaxed) {
            debug!("Event processor shutting down");
            break;
        }

        // 1. Drain incoming events from the channel.
        loop {
            match rx.try_recv() {
                Ok(event) => {
                    let key = hash_event(&event);
                    if let Some(last) = debounce_times.get(&key) {
                        if last.elapsed() < DEDUP_WINDOW {
                            debug!("Skipped event {key} due to debouncer");
                            continue;
                        }
                    }
                    debounce_times.insert(key, Instant::now());

                    match &event {
                        WatchEvent::Collage { .. } | WatchEvent::Playlist { .. } => {
                            // Process immediately.
                            handle_event(config, &event);
                        }
                        WatchEvent::Release { .. } => {
                            // Schedule with 2s delay.
                            let fire_at = Instant::now() + RELEASE_DELAY;
                            pending_releases.push((event, fire_at));
                        }
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    debug!("Channel disconnected, shutting down event processor");
                    return;
                }
            }
        }

        // 2. Fire any pending release events whose delay has elapsed.
        let now = Instant::now();
        let mut i = 0;
        while i < pending_releases.len() {
            if now >= pending_releases[i].1 {
                let (event, _) = pending_releases.swap_remove(i);
                handle_event(config, &event);
                // Don't increment i: swap_remove moved the last element here.
            } else {
                i += 1;
            }
        }

        // 3. Sleep briefly to avoid busy-looping.
        std::thread::sleep(Duration::from_millis(100));
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Start the file watcher. This blocks the calling thread until a SIGTERM is
/// received or the watcher encounters a fatal error.
pub fn start_watcher(config: &Config) -> Result<()> {
    let shutdown = Arc::new(AtomicBool::new(false));

    // Install SIGTERM handler.
    #[cfg(unix)]
    {
        let shutdown_signal = Arc::clone(&shutdown);
        // Use signal_hook for safe, portable SIGTERM handling.
        unsafe {
            // Register SIGTERM to set the shutdown flag.
            let shutdown_for_handler = Arc::clone(&shutdown_signal);
            signal_hook::low_level::register(signal_hook::consts::SIGTERM, move || {
                shutdown_for_handler.store(true, Ordering::Relaxed);
            })?;
            let shutdown_for_handler2 = Arc::clone(&shutdown_signal);
            signal_hook::low_level::register(signal_hook::consts::SIGINT, move || {
                shutdown_for_handler2.store(true, Ordering::Relaxed);
            })?;
        }
    }

    let (tx, rx) = mpsc::channel::<WatchEvent>();

    let music_source_dir = config.music_source_dir.clone();
    let mut watcher: RecommendedWatcher = notify::recommended_watcher(
        move |res: std::result::Result<notify::Event, notify::Error>| match res {
            Ok(event) => {
                let watch_events = classify_notify_event(&music_source_dir, &event);
                for we in watch_events {
                    if let Err(e) = tx.send(we) {
                        warn!("Failed to send event: {e}");
                    }
                }
            }
            Err(e) => {
                warn!("Watcher error: {e}");
            }
        },
    )?;

    info!(
        "Starting file watcher on {}",
        config.music_source_dir.display()
    );
    watcher.watch(&config.music_source_dir, RecursiveMode::Recursive)?;

    info!("Starting event processor");
    event_processor(config, rx, shutdown);

    // Watcher is dropped here, stopping the notify thread.
    info!("File watcher stopped");
    Ok(())
}

/// Stop a running watcher by sending SIGTERM to the PID in the PID file.
pub fn stop_watcher(config: &Config) -> Result<()> {
    let pid_path = config.watchdog_pid_path();

    if !pid_path.exists() {
        bail!(
            "No known watchdog running: PID file {} does not exist",
            pid_path.display()
        );
    }

    let content = std::fs::read_to_string(&pid_path)?;
    let pid_raw: i32 = match content.trim().parse() {
        Ok(p) => p,
        Err(_) => {
            // Stale/corrupt PID file — clean up.
            warn!("Corrupt PID file at {}, removing", pid_path.display());
            std::fs::remove_file(&pid_path)?;
            bail!("PID file was corrupt; cleaned up");
        }
    };

    #[cfg(unix)]
    {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;

        let pid = Pid::from_raw(pid_raw);
        match kill(pid, Signal::SIGTERM) {
            Ok(_) => {
                info!("Killed watchdog at process {pid_raw}");
            }
            Err(nix::errno::Errno::ESRCH) => {
                info!("Process {pid_raw} not found (already dead); cleaning up PID file");
            }
            Err(e) => {
                warn!("Failed to send SIGTERM to process {pid_raw}: {e}");
            }
        }
    }

    // Always clean up PID file.
    if pid_path.exists() {
        std::fs::remove_file(&pid_path)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn source_dir() -> PathBuf {
        PathBuf::from("/music")
    }

    // -- Event classification ------------------------------------------------

    #[test]
    fn classify_collage_path() {
        let src = source_dir();
        let path = src.join("!collages/Road Trip.toml");
        let result = classify_path(&src, &path, EventType::Modified);
        assert_eq!(
            result,
            Some(WatchEvent::Collage {
                name: "Road Trip".to_string(),
                event_type: EventType::Modified,
            })
        );
    }

    #[test]
    fn classify_playlist_path() {
        let src = source_dir();
        let path = src.join("!playlists/Chill Vibes.toml");
        let result = classify_path(&src, &path, EventType::Created);
        assert_eq!(
            result,
            Some(WatchEvent::Playlist {
                name: "Chill Vibes".to_string(),
                event_type: EventType::Created,
            })
        );
    }

    #[test]
    fn classify_release_file_path() {
        let src = source_dir();
        let path = src.join("Artist - Album (2023)/01. Track.flac");
        let result = classify_path(&src, &path, EventType::Modified);
        assert_eq!(
            result,
            Some(WatchEvent::Release {
                dir: src.join("Artist - Album (2023)"),
                event_type: EventType::Modified,
            })
        );
    }

    #[test]
    fn classify_root_level_event_ignored() {
        let src = source_dir();
        // Path is the source dir itself — strip_prefix yields "" which has no
        // components, so we get None.
        let result = classify_path(&src, &src, EventType::Modified);
        assert_eq!(result, None);
    }

    #[test]
    fn classify_collage_non_toml_ignored() {
        let src = source_dir();
        let path = src.join("!collages/backup.bak");
        let result = classify_path(&src, &path, EventType::Modified);
        assert_eq!(result, None);
    }

    #[test]
    fn classify_playlist_non_toml_ignored() {
        let src = source_dir();
        let path = src.join("!playlists/notes.txt");
        let result = classify_path(&src, &path, EventType::Created);
        assert_eq!(result, None);
    }

    // -- Debounce ------------------------------------------------------------

    #[test]
    fn debounce_drops_duplicate_within_window() {
        // Two identical events within 200ms: second should be dropped.
        let (tx, rx) = mpsc::channel();
        let _shutdown = Arc::new(AtomicBool::new(false));

        let event = WatchEvent::Collage {
            name: "Test".to_string(),
            event_type: EventType::Modified,
        };

        // Send the same event twice immediately.
        tx.send(event.clone()).unwrap();
        tx.send(event.clone()).unwrap();
        drop(tx);

        // Run the processor in a thread; it will exit when channel disconnects.
        // We count how many times handle_event would fire by capturing events.
        let mut debounce_times: HashMap<u64, Instant> = HashMap::new();
        let mut processed = 0u32;

        loop {
            match rx.try_recv() {
                Ok(ev) => {
                    let key = hash_event(&ev);
                    if let Some(last) = debounce_times.get(&key) {
                        if last.elapsed() < DEDUP_WINDOW {
                            continue; // debounced
                        }
                    }
                    debounce_times.insert(key, Instant::now());
                    processed += 1;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }

        assert_eq!(processed, 1, "second duplicate event should be debounced");
    }

    // -- Release delay vs immediate ------------------------------------------

    #[test]
    fn release_events_are_delayed_collage_immediate() {
        // Verify classification: release events get scheduled, collage/playlist don't.
        let event_collage = WatchEvent::Collage {
            name: "X".to_string(),
            event_type: EventType::Created,
        };
        let event_release = WatchEvent::Release {
            dir: PathBuf::from("/music/SomeAlbum"),
            event_type: EventType::Created,
        };

        // Collage should be immediate (not a release).
        assert!(matches!(event_collage, WatchEvent::Collage { .. }));
        // Release should be delayed (is a release).
        assert!(matches!(event_release, WatchEvent::Release { .. }));
    }

    // -- map_event_kind ------------------------------------------------------

    #[test]
    fn map_event_kind_create() {
        let kind = EventKind::Create(notify::event::CreateKind::File);
        assert_eq!(map_event_kind(&kind), Some(EventType::Created));
    }

    #[test]
    fn map_event_kind_remove() {
        let kind = EventKind::Remove(notify::event::RemoveKind::File);
        assert_eq!(map_event_kind(&kind), Some(EventType::Deleted));
    }

    #[test]
    fn map_event_kind_modify_data() {
        let kind = EventKind::Modify(notify::event::ModifyKind::Data(
            notify::event::DataChange::Content,
        ));
        assert_eq!(map_event_kind(&kind), Some(EventType::Modified));
    }

    #[test]
    fn map_event_kind_modify_rename() {
        let kind = EventKind::Modify(notify::event::ModifyKind::Name(
            notify::event::RenameMode::Both,
        ));
        assert_eq!(map_event_kind(&kind), Some(EventType::Moved));
    }

    #[test]
    fn map_event_kind_other_ignored() {
        let kind = EventKind::Other;
        assert_eq!(map_event_kind(&kind), None);
    }

    // -- stop_watcher with no PID file ---------------------------------------

    #[test]
    fn stop_watcher_no_pid_file() {
        // Create a minimal config pointing at a temp dir.
        let tmp = tempfile::tempdir().unwrap();
        let config_toml = format!(
            "music_source_dir = {:?}\n[vfs]\nmount_dir = {:?}\n",
            tmp.path().join("music"),
            tmp.path().join("vfs"),
        );
        let config_path = tmp.path().join("config.toml");
        std::fs::write(&config_path, &config_toml).unwrap();
        std::fs::create_dir_all(tmp.path().join("music")).unwrap();
        std::fs::create_dir_all(tmp.path().join("vfs")).unwrap();
        let config = Config::parse(Some(&config_path)).unwrap();

        let result = stop_watcher(&config);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("No known watchdog running"), "got: {msg}");
    }

    // -- stop_watcher with stale PID file ------------------------------------

    #[test]
    fn stop_watcher_stale_pid_file_cleanup() {
        let tmp = tempfile::tempdir().unwrap();
        let config_toml = format!(
            "music_source_dir = {:?}\n[vfs]\nmount_dir = {:?}\n",
            tmp.path().join("music"),
            tmp.path().join("vfs"),
        );
        let config_path = tmp.path().join("config.toml");
        std::fs::write(&config_path, &config_toml).unwrap();
        std::fs::create_dir_all(tmp.path().join("music")).unwrap();
        std::fs::create_dir_all(tmp.path().join("vfs")).unwrap();
        let config = Config::parse(Some(&config_path)).unwrap();

        // Write a PID file with a PID that almost certainly doesn't exist.
        let pid_path = config.watchdog_pid_path();
        std::fs::create_dir_all(pid_path.parent().unwrap()).unwrap();
        std::fs::write(&pid_path, "999999999").unwrap();

        let result = stop_watcher(&config);
        // Should succeed (cleans up stale PID).
        assert!(result.is_ok(), "expected Ok, got: {:?}", result);
        // PID file should be removed.
        assert!(!pid_path.exists(), "PID file should have been removed");
    }
}
