//! Shared mutable state types for the Rose Virtual Filesystem.
//!
//! All types here are designed to be wrapped in `Arc<RwLock<T>>` for thread-safe
//! access from the FUSE filesystem implementation.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::hash::Hash;
use std::path::{Path, PathBuf};
use std::time::Instant;

use thiserror::Error;

// ---------------------------------------------------------------------------
// TTLCache<K, V>
// ---------------------------------------------------------------------------

/// A dictionary with a time-to-live (TTL) for each key/value pair.
///
/// After the TTL passes, the key/value pair is no longer accessible via `get`.
/// No automatic eviction is performed (matches the Python implementation).
/// On `get`, the TTL is refreshed (timestamp updated) — matches Python behavior.
pub struct TTLCache<K, V> {
    ttl_seconds: u64,
    backing: HashMap<K, (V, Instant)>,
}

impl<K: Eq + Hash, V> TTLCache<K, V> {
    /// Create a new TTLCache with the given TTL in seconds.
    pub fn new(ttl_seconds: u64) -> Self {
        Self {
            ttl_seconds,
            backing: HashMap::new(),
        }
    }

    /// Insert a key/value pair, setting the TTL timestamp to now.
    pub fn insert(&mut self, key: K, value: V) {
        self.backing.insert(key, (value, Instant::now()));
    }

    /// Get a reference to the value for `key`, refreshing the TTL.
    /// Returns `None` if the key is absent or expired.
    pub fn get(&mut self, key: &K) -> Option<&V>
    where
        K: Clone,
    {
        let (val, timestamp) = self.backing.get_mut(key)?;
        if timestamp.elapsed().as_secs() > self.ttl_seconds {
            return None;
        }
        // Refresh TTL on access (matches Python virtualfs.py:152).
        *timestamp = Instant::now();
        Some(val)
    }

    /// Remove a key/value pair.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.backing.remove(key).map(|(v, _)| v)
    }

    /// Check if a non-expired entry exists for `key`.
    pub fn contains(&self, key: &K) -> bool {
        match self.backing.get(key) {
            Some((_, timestamp)) => timestamp.elapsed().as_secs() <= self.ttl_seconds,
            None => false,
        }
    }
}

// ---------------------------------------------------------------------------
// FileHandleManager
// ---------------------------------------------------------------------------

/// Error type for VFS state operations.
#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum VfsStateError {
    #[error("Bad file descriptor: unknown rose FH {0}")]
    BadFileDescriptor(u64),

    #[error("INode not found: {0}")]
    INodeNotFound(u64),

    #[error("Invalid name encoding")]
    InvalidEncoding,
}

/// Generates and manages file handles for the virtual filesystem.
///
/// Counter starts at 10. Reserved handle 9 = `/dev/null` sentinel.
/// Wraps at 10,000 with `max(10, ...)` to avoid reserved handles 0-9.
///
/// **Bug fix:** The Python implementation has `self._state + 1 % 10_000` which,
/// due to operator precedence, evaluates as `self._state + (1 % 10_000)` = `self._state + 1`,
/// meaning it never wraps. The Rust version correctly uses `(self.state + 1) % 10_000`.
#[allow(dead_code)]
pub struct FileHandleManager {
    state: u64,
    /// Sentinel file handle that acts as `/dev/null`.
    pub dev_null: u64,
    /// Mapping from rose VFS file handles to host OS file handles.
    rose_to_host: HashMap<u64, i32>,
}

impl Default for FileHandleManager {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
impl FileHandleManager {
    pub fn new() -> Self {
        Self {
            state: 10,
            dev_null: 9,
            rose_to_host: HashMap::new(),
        }
    }

    /// Generate the next file handle.
    /// Returns `max(10, (self.state + 1) % 10_000)`.
    pub fn next(&mut self) -> u64 {
        self.state = std::cmp::max(10, (self.state + 1) % 10_000);
        self.state
    }

    /// Wrap a host OS file handle: generate a rose FH and store the mapping.
    pub fn wrap_host(&mut self, host_fh: i32) -> u64 {
        let rose_fh = self.next();
        self.rose_to_host.insert(rose_fh, host_fh);
        rose_fh
    }

    /// Unwrap a rose FH back to the host OS file handle.
    /// Returns `Err(VfsStateError::BadFileDescriptor)` if the FH is unknown.
    pub fn unwrap_host(&self, rose_fh: u64) -> Result<i32, VfsStateError> {
        self.rose_to_host
            .get(&rose_fh)
            .copied()
            .ok_or(VfsStateError::BadFileDescriptor(rose_fh))
    }

    /// Remove a rose FH from the mapping (on close/release).
    pub fn release(&mut self, rose_fh: u64) {
        self.rose_to_host.remove(&rose_fh);
    }
}

// ---------------------------------------------------------------------------
// INodeMapper
// ---------------------------------------------------------------------------

/// Bidirectional map between inodes and paths.
///
/// Root inode = `fuser::FUSE_ROOT_ID` (1).
/// Counter increments to infinity (no wrapping), matching Python behavior.
#[allow(dead_code)]
pub struct INodeMapper {
    inode_to_path: HashMap<u64, PathBuf>,
    path_to_inode: HashMap<String, u64>,
    next_inode_ctr: u64,
}

#[allow(dead_code)]
impl INodeMapper {
    pub fn new() -> Self {
        let mut inode_to_path = HashMap::new();
        let mut path_to_inode = HashMap::new();
        inode_to_path.insert(fuser::FUSE_ROOT_ID, PathBuf::from("/"));
        path_to_inode.insert("/".to_string(), fuser::FUSE_ROOT_ID);
        Self {
            inode_to_path,
            path_to_inode,
            next_inode_ctr: fuser::FUSE_ROOT_ID + 1,
        }
    }

    fn next_inode(&mut self) -> u64 {
        let cur = self.next_inode_ctr;
        self.next_inode_ctr += 1;
        cur
    }

    /// Get the path for an inode. If `name` is provided and the inode refers
    /// to a directory, the name is appended to the path.
    ///
    /// Returns `Err` if the inode is unknown.
    pub fn get_path(&self, inode: u64, name: Option<&OsStr>) -> Result<PathBuf, VfsStateError> {
        let path = self
            .inode_to_path
            .get(&inode)
            .ok_or(VfsStateError::INodeNotFound(inode))?;

        match name {
            None => Ok(path.clone()),
            Some(n) if n == "." => Ok(path.clone()),
            Some(n) if n == ".." => Ok(path.parent().unwrap_or(path).to_path_buf()),
            Some(n) => Ok(path.join(n)),
        }
    }

    /// Get or assign an inode for the given path. If the path has been seen
    /// before, returns the cached inode. Otherwise, generates a new one.
    pub fn calc_inode(&mut self, path: &Path) -> u64 {
        let spath = path.to_string_lossy().to_string();
        if let Some(&inode) = self.path_to_inode.get(&spath) {
            return inode;
        }
        let inode = self.next_inode();
        self.path_to_inode.insert(spath, inode);
        self.inode_to_path.insert(inode, path.to_path_buf());
        inode
    }

    /// Remove a path and its associated inode from the mapping.
    pub fn remove_path(&mut self, path: &Path) {
        let spath = path.to_string_lossy().to_string();
        if let Some(inode) = self.path_to_inode.remove(&spath) {
            self.inode_to_path.remove(&inode);
        }
    }

    /// Rename a path: move its inode mapping from `old` to `new`.
    pub fn rename_path(&mut self, old: &Path, new: &Path) {
        let sold = old.to_string_lossy().to_string();
        let snew = new.to_string_lossy().to_string();
        if let Some(inode) = self.path_to_inode.remove(&sold) {
            self.inode_to_path.insert(inode, new.to_path_buf());
            self.path_to_inode.insert(snew, inode);
        }
    }
}

// ---------------------------------------------------------------------------
// EntryAttrs
// ---------------------------------------------------------------------------

/// Portable file/directory attributes for the virtual filesystem.
#[derive(Debug, Clone)]
pub struct EntryAttrs {
    pub st_mode: u32,
    pub st_nlink: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub st_size: u64,
    pub st_atime: i64,
    pub st_mtime: i64,
    pub st_ctime: i64,
    pub st_ino: u64,
}

impl EntryAttrs {
    /// Create default attributes for a directory or file.
    /// If `realpath` is provided, stat the real file for size/times.
    pub fn stat(mode: &str, realpath: Option<&Path>) -> Self {
        let uid = unsafe { libc::getuid() };
        let gid = unsafe { libc::getgid() };

        let (st_mode, perm) = if mode == "dir" {
            (libc::S_IFDIR, 0o755u32)
        } else {
            (libc::S_IFREG, 0o644u32)
        };

        let mut attrs = Self {
            st_mode: st_mode | perm,
            st_nlink: 4,
            st_uid: uid,
            st_gid: gid,
            st_size: 4096,
            st_atime: 0,
            st_mtime: 0,
            st_ctime: 0,
            st_ino: 0,
        };

        if let Some(p) = realpath {
            if let Ok(meta) = std::fs::metadata(p) {
                attrs.st_size = meta.len();
                // Use std::os::unix::fs::MetadataExt for precise timestamps.
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    attrs.st_atime = meta.atime();
                    attrs.st_mtime = meta.mtime();
                    attrs.st_ctime = meta.ctime();
                }
            }
        }

        attrs
    }
}

// ---------------------------------------------------------------------------
// FileCreationSpecialOp and CoverArtTarget
// ---------------------------------------------------------------------------

/// Target entity for a cover art operation.
#[derive(Debug, Clone)]
pub enum CoverArtTarget {
    Release(String),
    Playlist(String),
}

/// Represents an in-flight file creation operation that requires special
/// handling across multiple syscalls (open → write → release).
#[derive(Debug, Clone)]
pub enum FileCreationSpecialOp {
    /// A track file being written to add it to a playlist.
    AddTrackToPlaylist {
        playlist: String,
        ext: String,
        data: Vec<u8>,
    },
    /// A cover art image being written.
    NewCoverArt {
        entity: CoverArtTarget,
        ext: String,
        data: Vec<u8>,
    },
}

// ---------------------------------------------------------------------------
// ReaddirEntry
// ---------------------------------------------------------------------------

/// A single entry from a readdir result, cached for subsequent readdir calls.
#[derive(Debug, Clone)]
pub struct ReaddirEntry {
    pub parent_inode: u64,
    pub name: Vec<u8>,
    pub attrs: EntryAttrs,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    // -- TTLCache tests --

    #[test]
    fn ttl_cache_insert_and_get() {
        let mut cache = TTLCache::new(5);
        cache.insert("hello".to_string(), 42);
        assert_eq!(cache.get(&"hello".to_string()), Some(&42));
    }

    #[test]
    fn ttl_cache_missing_key_returns_none() {
        let mut cache: TTLCache<String, i32> = TTLCache::new(5);
        assert_eq!(cache.get(&"missing".to_string()), None);
    }

    #[test]
    fn ttl_cache_expired_entry_returns_none() {
        let mut cache = TTLCache::new(0); // 0 second TTL
        cache.insert("key".to_string(), 1);
        // Sleep just past the TTL.
        thread::sleep(Duration::from_millis(1100));
        assert_eq!(cache.get(&"key".to_string()), None);
        assert!(!cache.contains(&"key".to_string()));
    }

    #[test]
    fn ttl_cache_get_refreshes_ttl() {
        let mut cache = TTLCache::new(1);
        cache.insert("key".to_string(), 100);
        // Access before expiry to refresh.
        thread::sleep(Duration::from_millis(600));
        assert_eq!(cache.get(&"key".to_string()), Some(&100)); // refreshes TTL
                                                               // Sleep again — should still be valid because we refreshed.
        thread::sleep(Duration::from_millis(600));
        assert_eq!(cache.get(&"key".to_string()), Some(&100));
    }

    #[test]
    fn ttl_cache_remove() {
        let mut cache = TTLCache::new(60);
        cache.insert("key".to_string(), 1);
        cache.remove(&"key".to_string());
        assert_eq!(cache.get(&"key".to_string()), None);
    }

    #[test]
    fn ttl_cache_contains() {
        let mut cache = TTLCache::new(60);
        assert!(!cache.contains(&"x".to_string()));
        cache.insert("x".to_string(), true);
        assert!(cache.contains(&"x".to_string()));
    }

    // -- FileHandleManager tests --

    #[test]
    fn fh_manager_next_starts_at_11() {
        let mut fhm = FileHandleManager::new();
        assert_eq!(fhm.next(), 11);
    }

    #[test]
    fn fh_manager_next_increments() {
        let mut fhm = FileHandleManager::new();
        let a = fhm.next();
        let b = fhm.next();
        assert_eq!(b, a + 1);
    }

    #[test]
    fn fh_manager_wrapping_at_10000() {
        let mut fhm = FileHandleManager::new();
        // Manually set state close to the wrapping point.
        fhm.state = 9998;
        assert_eq!(fhm.next(), 9999);
        // Next: (9999 + 1) % 10_000 = 0, max(10, 0) = 10
        assert_eq!(fhm.next(), 10);
        // Next: (10 + 1) % 10_000 = 11, max(10, 11) = 11
        assert_eq!(fhm.next(), 11);
    }

    #[test]
    fn fh_manager_min_is_10() {
        let mut fhm = FileHandleManager::new();
        // Force state to 9999 so next wraps to 0 → clamped to 10.
        fhm.state = 9999;
        let fh = fhm.next();
        assert!(fh >= 10, "File handle must be >= 10, got {fh}");
    }

    #[test]
    fn fh_manager_dev_null() {
        let fhm = FileHandleManager::new();
        assert_eq!(fhm.dev_null, 9);
    }

    #[test]
    fn fh_manager_wrap_unwrap_roundtrip() {
        let mut fhm = FileHandleManager::new();
        let host_fh: i32 = 42;
        let rose_fh = fhm.wrap_host(host_fh);
        assert_eq!(fhm.unwrap_host(rose_fh).unwrap(), host_fh);
    }

    #[test]
    fn fh_manager_unwrap_unknown_returns_error() {
        let fhm = FileHandleManager::new();
        assert!(fhm.unwrap_host(99999).is_err());
    }

    #[test]
    fn fh_manager_release_removes_mapping() {
        let mut fhm = FileHandleManager::new();
        let rose_fh = fhm.wrap_host(42);
        fhm.release(rose_fh);
        assert!(fhm.unwrap_host(rose_fh).is_err());
    }

    // -- INodeMapper tests --

    #[test]
    fn inode_mapper_root() {
        let mapper = INodeMapper::new();
        let path = mapper.get_path(fuser::FUSE_ROOT_ID, None).unwrap();
        assert_eq!(path, PathBuf::from("/"));
    }

    #[test]
    fn inode_mapper_calc_returns_same_inode_for_same_path() {
        let mut mapper = INodeMapper::new();
        let path = Path::new("/foo/bar");
        let ino1 = mapper.calc_inode(path);
        let ino2 = mapper.calc_inode(path);
        assert_eq!(ino1, ino2);
    }

    #[test]
    fn inode_mapper_different_paths_get_different_inodes() {
        let mut mapper = INodeMapper::new();
        let ino1 = mapper.calc_inode(Path::new("/a"));
        let ino2 = mapper.calc_inode(Path::new("/b"));
        assert_ne!(ino1, ino2);
    }

    #[test]
    fn inode_mapper_get_path_calc_inode_roundtrip() {
        let mut mapper = INodeMapper::new();
        let path = Path::new("/music/album");
        let ino = mapper.calc_inode(path);
        let recovered = mapper.get_path(ino, None).unwrap();
        assert_eq!(recovered, path);
    }

    #[test]
    fn inode_mapper_get_path_with_name() {
        let mut mapper = INodeMapper::new();
        let dir = Path::new("/music");
        let ino = mapper.calc_inode(dir);
        let child = mapper
            .get_path(ino, Some(OsStr::new("track.flac")))
            .unwrap();
        assert_eq!(child, PathBuf::from("/music/track.flac"));
    }

    #[test]
    fn inode_mapper_get_path_dot() {
        let mut mapper = INodeMapper::new();
        let dir = Path::new("/music");
        let ino = mapper.calc_inode(dir);
        let same = mapper.get_path(ino, Some(OsStr::new("."))).unwrap();
        assert_eq!(same, PathBuf::from("/music"));
    }

    #[test]
    fn inode_mapper_get_path_dotdot() {
        let mut mapper = INodeMapper::new();
        let dir = Path::new("/music/album");
        let ino = mapper.calc_inode(dir);
        let parent = mapper.get_path(ino, Some(OsStr::new(".."))).unwrap();
        assert_eq!(parent, PathBuf::from("/music"));
    }

    #[test]
    fn inode_mapper_unknown_inode_returns_error() {
        let mapper = INodeMapper::new();
        assert!(mapper.get_path(999999, None).is_err());
    }

    #[test]
    fn inode_mapper_remove_path() {
        let mut mapper = INodeMapper::new();
        let path = Path::new("/to/remove");
        let ino = mapper.calc_inode(path);
        mapper.remove_path(path);
        assert!(mapper.get_path(ino, None).is_err());
    }

    #[test]
    fn inode_mapper_remove_nonexistent_path_is_noop() {
        let mut mapper = INodeMapper::new();
        // Should not panic.
        mapper.remove_path(Path::new("/nonexistent"));
    }

    #[test]
    fn inode_mapper_rename_path() {
        let mut mapper = INodeMapper::new();
        let old = Path::new("/old/path");
        let new = Path::new("/new/path");
        let ino = mapper.calc_inode(old);
        mapper.rename_path(old, new);
        // Old path should be gone.
        let ino_new = mapper.calc_inode(new);
        assert_eq!(ino, ino_new, "renamed path should keep the same inode");
        // The old path should yield a new inode (since it was removed).
        let ino_old = mapper.calc_inode(old);
        assert_ne!(ino, ino_old);
    }

    // -- EntryAttrs tests --

    #[test]
    fn entry_attrs_dir_mode() {
        let attrs = EntryAttrs::stat("dir", None);
        // S_IFDIR | 0755
        assert_ne!(attrs.st_mode & libc::S_IFDIR as u32, 0);
        assert_eq!(attrs.st_mode & 0o777, 0o755);
    }

    #[test]
    fn entry_attrs_file_mode() {
        let attrs = EntryAttrs::stat("file", None);
        // S_IFREG | 0644
        assert_ne!(attrs.st_mode & libc::S_IFREG as u32, 0);
        assert_eq!(attrs.st_mode & 0o777, 0o644);
    }

    #[test]
    fn entry_attrs_default_size() {
        let attrs = EntryAttrs::stat("dir", None);
        assert_eq!(attrs.st_size, 4096);
    }

    #[test]
    fn entry_attrs_real_path() {
        // Use Cargo.toml as a known existing file.
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let attrs = EntryAttrs::stat("file", Some(&path));
        assert!(attrs.st_size > 0, "real file should have non-zero size");
    }
}
