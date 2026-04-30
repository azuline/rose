pub mod logical;
mod state;
pub mod virtualfs;

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clap::Parser;
use fuser::{
    FileAttr, FileType, Filesystem, MountOption, ReplyAttr, ReplyCreate, ReplyData, ReplyDirectory,
    ReplyEmpty, ReplyEntry, ReplyOpen, ReplyStatfs, ReplyWrite, ReplyXattr, Request,
};
use rand::Rng;
use tracing::{debug, info, warn};

use logical::RoseLogicalCore;
use rose_core::cache::STORED_DATA_FILE_REGEX;
use rose_core::config::Config;
use state::{EntryAttrs, FileHandleManager, INodeMapper, ReaddirEntry, TTLCache};
use virtualfs::VirtualPath;

/// Rose Virtual Filesystem — presents a music library as a virtual directory tree.
#[derive(Parser, Debug)]
#[command(name = "rose-vfs", version, about)]
struct Cli {
    /// Mount point directory.
    #[arg(short, long)]
    mount: PathBuf,

    /// Path to the Rose configuration file.
    #[arg(short, long)]
    config: Option<PathBuf>,
}

// ---------------------------------------------------------------------------
// EntryAttrs → FileAttr conversion
// ---------------------------------------------------------------------------

/// Convert an `EntryAttrs` into a `fuser::FileAttr`.
fn entry_attrs_to_file_attr(attrs: &EntryAttrs, uid: u32, gid: u32) -> FileAttr {
    let kind = if attrs.st_mode & libc::S_IFDIR != 0 {
        FileType::Directory
    } else {
        FileType::RegularFile
    };
    let perm = (attrs.st_mode & 0o777) as u16;

    let atime = if attrs.st_atime > 0 {
        UNIX_EPOCH + Duration::from_secs(attrs.st_atime as u64)
    } else {
        SystemTime::now()
    };
    let mtime = if attrs.st_mtime > 0 {
        UNIX_EPOCH + Duration::from_secs(attrs.st_mtime as u64)
    } else {
        SystemTime::now()
    };
    let ctime = if attrs.st_ctime > 0 {
        UNIX_EPOCH + Duration::from_secs(attrs.st_ctime as u64)
    } else {
        SystemTime::now()
    };

    FileAttr {
        ino: attrs.st_ino,
        size: attrs.st_size,
        blocks: attrs.st_size.div_ceil(512),
        atime,
        mtime,
        ctime,
        crtime: UNIX_EPOCH,
        kind,
        perm,
        nlink: attrs.st_nlink,
        uid,
        gid,
        rdev: 0,
        blksize: 512,
        flags: 0,
    }
}

// ---------------------------------------------------------------------------
// RoseFs — the FUSE filesystem
// ---------------------------------------------------------------------------

/// The FUSE filesystem implementation. Wraps `RoseLogicalCore` with inode
/// management, caching, and FUSE-level translation.
struct RoseFs {
    rose: RoseLogicalCore,
    inodes: INodeMapper,
    fhandler: FileHandleManager,
    generation: u64,
    uid: u32,
    gid: u32,
    /// 1-second TTL cache for getattr results, populated by readdir.
    getattr_cache: TTLCache<u64, FileAttr>,
    /// 1-second TTL cache for lookup results, populated by readdir.
    lookup_cache: TTLCache<(u64, Vec<u8>), FileAttr>,
    /// Stores opendir results keyed by FH for subsequent readdir calls.
    readdir_cache: HashMap<u64, Vec<ReaddirEntry>>,
    /// Ghost files: pretend files exist for 5s after creation.
    ghost_existing_files: TTLCache<String, bool>,
    /// Ghost directories for in-progress collage additions (5s TTL).
    in_progress_collage_additions: TTLCache<String, bool>,
}

/// Default entry timeout for FUSE entries (30 seconds).
const ENTRY_TTL: Duration = Duration::from_secs(30);

/// Default attr timeout for FUSE attributes (30 seconds).
const ATTR_TTL: Duration = Duration::from_secs(30);

impl RoseFs {
    fn new(config: Config) -> Self {
        let uid = unsafe { libc::getuid() };
        let gid = unsafe { libc::getgid() };
        let generation = rand::thread_rng().gen_range(0..1_000_000);
        Self {
            rose: RoseLogicalCore::new(config),
            inodes: INodeMapper::new(),
            fhandler: FileHandleManager::new(),
            generation,
            uid,
            gid,
            getattr_cache: TTLCache::new(1),
            lookup_cache: TTLCache::new(1),
            readdir_cache: HashMap::new(),
            ghost_existing_files: TTLCache::new(5),
            in_progress_collage_additions: TTLCache::new(5),
        }
    }

    /// Replace both getattr and lookup caches with fresh empty instances.
    /// Called after any mutation operation.
    fn reset_getattr_caches(&mut self) {
        self.getattr_cache = TTLCache::new(1);
        self.lookup_cache = TTLCache::new(1);
    }

    /// Convert `EntryAttrs` to `FileAttr`, setting inode.
    fn make_file_attr(&self, attrs: &EntryAttrs) -> FileAttr {
        entry_attrs_to_file_attr(attrs, self.uid, self.gid)
    }

    /// Build a directory FileAttr for the given inode.
    fn dir_attr(&self, ino: u64) -> FileAttr {
        let now = SystemTime::now();
        FileAttr {
            ino,
            size: 4096,
            blocks: 8,
            atime: now,
            mtime: now,
            ctime: now,
            crtime: UNIX_EPOCH,
            kind: FileType::Directory,
            perm: 0o755,
            nlink: 4,
            uid: self.uid,
            gid: self.gid,
            rdev: 0,
            blksize: 512,
            flags: 0,
        }
    }

    /// Build a regular file FileAttr for the given inode.
    fn file_attr(&self, ino: u64) -> FileAttr {
        let now = SystemTime::now();
        FileAttr {
            ino,
            size: 4096,
            blocks: 8,
            atime: now,
            mtime: now,
            ctime: now,
            crtime: UNIX_EPOCH,
            kind: FileType::RegularFile,
            perm: 0o644,
            nlink: 4,
            uid: self.uid,
            gid: self.gid,
            rdev: 0,
            blksize: 512,
            flags: 0,
        }
    }
}

impl Filesystem for RoseFs {
    // -----------------------------------------------------------------------
    // getattr
    // -----------------------------------------------------------------------
    fn getattr(&mut self, _req: &Request<'_>, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        debug!("FUSE: getattr(ino={})", ino);

        // Check getattr cache first.
        if let Some(cached) = self.getattr_cache.get(&ino) {
            debug!("FUSE: getattr cache hit for ino={}", ino);
            let attr = *cached;
            reply.attr(&ATTR_TTL, &attr);
            return;
        }

        // Resolve inode to path.
        let spath = match self.inodes.get_path(ino, None) {
            Ok(p) => p,
            Err(_) => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        debug!("FUSE: getattr ino={} -> path={:?}", ino, spath);

        // Ghost file check.
        if self
            .ghost_existing_files
            .contains(&spath.to_string_lossy().to_string())
        {
            debug!("FUSE: getattr resolved as ghost existing file: {:?}", spath);
            let mut attrs = EntryAttrs::stat("file", None);
            attrs.st_ino = ino;
            reply.attr(&ATTR_TTL, &self.make_file_attr(&attrs));
            return;
        }

        // In-progress collage addition check.
        if self
            .in_progress_collage_additions
            .contains(&spath.to_string_lossy().to_string())
        {
            debug!(
                "FUSE: getattr resolved as in-progress collage addition: {:?}",
                spath
            );
            let mut attrs = EntryAttrs::stat("dir", None);
            attrs.st_ino = ino;
            reply.attr(&ATTR_TTL, &self.make_file_attr(&attrs));
            return;
        }

        // Parse VirtualPath and call logical core.
        let vpath = match VirtualPath::parse(&spath) {
            Ok(vp) => vp,
            Err(e) => {
                reply.error(e);
                return;
            }
        };
        debug!("FUSE: getattr parsed path={:?} -> {:?}", spath, vpath);

        match self.rose.getattr(&vpath) {
            Ok(mut attrs) => {
                attrs.st_ino = ino;
                let file_attr = self.make_file_attr(&attrs);
                reply.attr(&ATTR_TTL, &file_attr);
            }
            Err(e) => {
                reply.error(e);
            }
        }
    }

    // -----------------------------------------------------------------------
    // lookup
    // -----------------------------------------------------------------------
    fn lookup(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let name_bytes = name.as_encoded_bytes().to_vec();
        debug!("FUSE: lookup(parent={}, name={:?})", parent, name);

        // Early exit for known bad filenames.
        if name_bytes == b":" || name_bytes == b"." || name_bytes == b".." || name_bytes.is_empty()
        {
            reply.error(libc::ENOENT);
            return;
        }

        // Check lookup cache.
        let cache_key = (parent, name_bytes.clone());
        if let Some(cached) = self.lookup_cache.get(&cache_key) {
            debug!(
                "FUSE: lookup cache hit for parent={} name={:?}",
                parent, name
            );
            let attr = *cached;
            reply.entry(&ENTRY_TTL, &attr, self.generation);
            return;
        }

        // Resolve parent inode to path, append name.
        let spath = match self.inodes.get_path(parent, Some(name)) {
            Ok(p) => p,
            Err(_) => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let inode = self.inodes.calc_inode(&spath);
        debug!(
            "FUSE: lookup parent={} name={:?} -> path={:?} ino={}",
            parent, name, spath, inode
        );

        // Ghost file check.
        if self
            .ghost_existing_files
            .contains(&spath.to_string_lossy().to_string())
        {
            debug!("FUSE: lookup resolved as ghost existing file: {:?}", spath);
            let mut attrs = EntryAttrs::stat("file", None);
            attrs.st_ino = inode;
            let file_attr = self.make_file_attr(&attrs);
            reply.entry(&ENTRY_TTL, &file_attr, self.generation);
            return;
        }

        // In-progress collage addition: check parent path.
        if let Some(parent_path) = spath.parent() {
            if self
                .in_progress_collage_additions
                .contains(&parent_path.to_string_lossy().to_string())
            {
                debug!(
                    "FUSE: lookup resolved as in-progress collage addition: {:?}",
                    spath
                );
                reply.error(libc::ENOENT);
                return;
            }
        }

        // Parse VirtualPath and call logical core.
        let vpath = match VirtualPath::parse(&spath) {
            Ok(vp) => vp,
            Err(e) => {
                reply.error(e);
                return;
            }
        };
        debug!("FUSE: lookup parsed path={:?} -> {:?}", spath, vpath);

        match self.rose.getattr(&vpath) {
            Ok(mut attrs) => {
                attrs.st_ino = inode;
                let file_attr = self.make_file_attr(&attrs);
                reply.entry(&ENTRY_TTL, &file_attr, self.generation);
            }
            Err(e) => {
                reply.error(e);
            }
        }
    }

    // -----------------------------------------------------------------------
    // opendir
    // -----------------------------------------------------------------------
    fn opendir(&mut self, _req: &Request<'_>, ino: u64, _flags: i32, reply: ReplyOpen) {
        debug!("FUSE: opendir(ino={})", ino);
        let spath = match self.inodes.get_path(ino, None) {
            Ok(p) => p,
            Err(_) => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        debug!("FUSE: opendir ino={} -> path={:?}", ino, spath);

        // In-progress collage addition: return empty directory.
        if self
            .in_progress_collage_additions
            .contains(&spath.to_string_lossy().to_string())
        {
            debug!(
                "FUSE: opendir resolved as in-progress collage addition: {:?}",
                spath
            );
            let mut entries = Vec::new();
            for node_name in &[".", ".."] {
                let node_path = spath.join(node_name);
                let node_ino = self.inodes.calc_inode(&node_path);
                let mut attrs = EntryAttrs::stat("dir", None);
                attrs.st_ino = node_ino;
                entries.push(ReaddirEntry {
                    parent_inode: ino,
                    name: node_name.as_bytes().to_vec(),
                    attrs,
                });
            }
            let fh = self.fhandler.next();
            self.readdir_cache.insert(fh, entries);
            reply.opened(fh, 0);
            return;
        }

        let vpath = match VirtualPath::parse(&spath) {
            Ok(vp) => vp,
            Err(e) => {
                reply.error(e);
                return;
            }
        };
        debug!("FUSE: opendir parsed path={:?} -> {:?}", spath, vpath);

        match self.rose.readdir(&vpath) {
            Ok(dir_entries) => {
                let mut entries = Vec::with_capacity(dir_entries.len());
                for (namestr, attrs) in &dir_entries {
                    let child_path = spath.join(namestr);
                    let child_ino = self.inodes.calc_inode(&child_path);
                    let mut entry_attrs = attrs.clone();
                    entry_attrs.st_ino = child_ino;
                    entries.push(ReaddirEntry {
                        parent_inode: ino,
                        name: namestr.as_bytes().to_vec(),
                        attrs: entry_attrs,
                    });
                }
                let fh = self.fhandler.next();
                debug!(
                    "FUSE: opendir stored {} entries in readdir cache for fh={}",
                    entries.len(),
                    fh
                );
                self.readdir_cache.insert(fh, entries);
                reply.opened(fh, 0);
            }
            Err(e) => {
                reply.error(e);
            }
        }
    }

    // -----------------------------------------------------------------------
    // readdir
    // -----------------------------------------------------------------------
    fn readdir(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        debug!("FUSE: readdir(fh={}, offset={})", fh, offset);

        let entries = match self.readdir_cache.get(&fh) {
            Some(e) => e.clone(),
            None => {
                reply.ok();
                return;
            }
        };

        for (i, entry) in entries.iter().enumerate().skip(offset as usize) {
            let kind = if entry.attrs.st_mode & libc::S_IFDIR != 0 {
                FileType::Directory
            } else {
                FileType::RegularFile
            };

            // Populate getattr and lookup caches for each entry.
            let file_attr = self.make_file_attr(&entry.attrs);
            self.getattr_cache.insert(entry.attrs.st_ino, file_attr);
            self.lookup_cache
                .insert((entry.parent_inode, entry.name.clone()), file_attr);

            let name_os = OsStr::new(std::str::from_utf8(&entry.name).unwrap_or("?"));
            if reply.add(entry.attrs.st_ino, (i + 1) as i64, kind, name_os) {
                break;
            }
        }
        reply.ok();
    }

    // -----------------------------------------------------------------------
    // releasedir
    // -----------------------------------------------------------------------
    fn releasedir(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        fh: u64,
        _flags: i32,
        reply: ReplyEmpty,
    ) {
        debug!("FUSE: releasedir(fh={})", fh);
        self.readdir_cache.remove(&fh);
        reply.ok();
    }

    // -----------------------------------------------------------------------
    // open
    // -----------------------------------------------------------------------
    fn open(&mut self, _req: &Request<'_>, ino: u64, flags: i32, reply: ReplyOpen) {
        debug!("FUSE: open(ino={}, flags={})", ino, flags);
        let spath = match self.inodes.get_path(ino, None) {
            Ok(p) => p,
            Err(_) => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        debug!("FUSE: open ino={} -> path={:?}", ino, spath);

        let vpath = match VirtualPath::parse(&spath) {
            Ok(vp) => vp,
            Err(e) => {
                reply.error(e);
                return;
            }
        };
        debug!("FUSE: open parsed path={:?} -> {:?}", spath, vpath);

        // Black hole files written to an in-progress collage addition,
        // EXCEPT for the Rose datafile which is passed through.
        let spath_str = spath.to_string_lossy().to_string();
        if let Some(parent) = spath.parent() {
            let parent_str = parent.to_string_lossy().to_string();
            if self.in_progress_collage_additions.contains(&parent_str) {
                let is_rose_datafile = vpath.file.as_ref().is_some_and(|f| {
                    STORED_DATA_FILE_REGEX.is_match(f) && flags & libc::O_CREAT == libc::O_CREAT
                });
                if !is_rose_datafile {
                    debug!(
                        "FUSE: open resolved as in-progress collage addition: {:?}",
                        spath
                    );
                    self.ghost_existing_files.insert(spath_str, true);
                    reply.opened(self.fhandler.dev_null, 0);
                    return;
                }
            }
        }

        // Black hole "._*" files on macOS (extended attribute data).
        if let Some(ref file) = vpath.file {
            if file.starts_with("._") {
                self.ghost_existing_files.insert(spath_str.clone(), true);
                reply.opened(self.fhandler.dev_null, 0);
                return;
            }
        }

        match self.rose.open(&vpath, flags) {
            Ok(fh) => {
                // If O_CREAT, flag the filepath as a ghost file.
                if flags & libc::O_CREAT == libc::O_CREAT {
                    debug!("FUSE: setting {:?} as ghost existing file", spath);
                    self.ghost_existing_files.insert(spath_str, true);
                }
                reply.opened(fh, 0);
            }
            Err(e) => {
                reply.error(e);
            }
        }
    }

    // -----------------------------------------------------------------------
    // read
    // -----------------------------------------------------------------------
    fn read(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyData,
    ) {
        debug!("FUSE: read(fh={}, offset={}, size={})", fh, offset, size);

        // Dev null sentinel returns empty.
        if fh == self.fhandler.dev_null {
            debug!("FUSE: read matched dev_null sentinel");
            reply.data(&[]);
            return;
        }

        match self.rose.read(fh, offset, size) {
            Ok(data) => {
                reply.data(&data);
            }
            Err(e) => {
                reply.error(e);
            }
        }
    }

    // -----------------------------------------------------------------------
    // write
    // -----------------------------------------------------------------------
    fn write(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        fh: u64,
        offset: i64,
        data: &[u8],
        _write_flags: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyWrite,
    ) {
        debug!(
            "FUSE: write(fh={}, offset={}, len={})",
            fh,
            offset,
            data.len()
        );

        // Dev null sentinel: claim we wrote everything.
        if fh == self.fhandler.dev_null {
            debug!("FUSE: write matched dev_null sentinel");
            reply.written(data.len() as u32);
            return;
        }

        match self.rose.write(fh, offset, data) {
            Ok(n) => {
                reply.written(n);
            }
            Err(e) => {
                reply.error(e);
            }
        }
    }

    // -----------------------------------------------------------------------
    // release
    // -----------------------------------------------------------------------
    fn release(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        fh: u64,
        _flags: i32,
        _lock_owner: Option<u64>,
        _flush: bool,
        reply: ReplyEmpty,
    ) {
        debug!("FUSE: release(fh={})", fh);

        // Dev null sentinel: noop.
        if fh == self.fhandler.dev_null {
            debug!("FUSE: release matched dev_null sentinel");
            reply.ok();
            return;
        }

        match self.rose.release(fh) {
            Ok(()) => {
                reply.ok();
            }
            Err(e) => {
                reply.error(e);
            }
        }
    }

    // -----------------------------------------------------------------------
    // create
    // -----------------------------------------------------------------------
    fn create(
        &mut self,
        _req: &Request<'_>,
        parent: u64,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
        flags: i32,
        reply: ReplyCreate,
    ) {
        debug!(
            "FUSE: create(parent={}, name={:?}, flags={})",
            parent, name, flags
        );

        let path = match self.inodes.get_path(parent, Some(name)) {
            Ok(p) => p,
            Err(_) => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let inode = self.inodes.calc_inode(&path);
        debug!("FUSE: create resolved path={:?} inode={}", path, inode);

        let vpath = match VirtualPath::parse(&path) {
            Ok(vp) => vp,
            Err(e) => {
                reply.error(e);
                return;
            }
        };

        // Check in-progress collage additions.
        let path_str = path.to_string_lossy().to_string();
        if let Some(parent_path) = path.parent() {
            let parent_str = parent_path.to_string_lossy().to_string();
            if self.in_progress_collage_additions.contains(&parent_str) {
                let is_rose_datafile = vpath.file.as_ref().is_some_and(|f| {
                    STORED_DATA_FILE_REGEX.is_match(f) && flags & libc::O_CREAT == libc::O_CREAT
                });
                if !is_rose_datafile {
                    self.ghost_existing_files.insert(path_str, true);
                    let attr = self.file_attr(inode);
                    reply.created(
                        &ENTRY_TTL,
                        &attr,
                        self.generation,
                        self.fhandler.dev_null,
                        0,
                    );
                    return;
                }
            }
        }

        // Black hole "._*" files on macOS.
        if let Some(ref file) = vpath.file {
            if file.starts_with("._") {
                self.ghost_existing_files.insert(path_str, true);
                let attr = self.file_attr(inode);
                reply.created(
                    &ENTRY_TTL,
                    &attr,
                    self.generation,
                    self.fhandler.dev_null,
                    0,
                );
                return;
            }
        }

        match self.rose.open(&vpath, flags | libc::O_CREAT) {
            Ok(fh) => {
                self.reset_getattr_caches();
                self.ghost_existing_files.insert(path_str, true);
                let attr = self.file_attr(inode);
                reply.created(&ENTRY_TTL, &attr, self.generation, fh, 0);
            }
            Err(e) => {
                reply.error(e);
            }
        }
    }

    // -----------------------------------------------------------------------
    // unlink
    // -----------------------------------------------------------------------
    fn unlink(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        debug!("FUSE: unlink(parent={}, name={:?})", parent, name);
        let spath = match self.inodes.get_path(parent, Some(name)) {
            Ok(p) => p,
            Err(_) => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let vpath = match VirtualPath::parse(&spath) {
            Ok(vp) => vp,
            Err(e) => {
                reply.error(e);
                return;
            }
        };
        debug!("FUSE: unlink parsed path={:?} -> {:?}", spath, vpath);

        match self.rose.unlink(&vpath) {
            Ok(()) => {
                self.reset_getattr_caches();
                self.inodes.remove_path(&spath);
                self.ghost_existing_files
                    .remove(&spath.to_string_lossy().to_string());
                reply.ok();
            }
            Err(e) => {
                reply.error(e);
            }
        }
    }

    // -----------------------------------------------------------------------
    // mkdir
    // -----------------------------------------------------------------------
    fn mkdir(
        &mut self,
        _req: &Request<'_>,
        parent: u64,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
        reply: ReplyEntry,
    ) {
        debug!("FUSE: mkdir(parent={}, name={:?})", parent, name);
        let spath = match self.inodes.get_path(parent, Some(name)) {
            Ok(p) => p,
            Err(_) => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let vpath = match VirtualPath::parse(&spath) {
            Ok(vp) => vp,
            Err(e) => {
                reply.error(e);
                return;
            }
        };
        debug!("FUSE: mkdir parsed path={:?} -> {:?}", spath, vpath);

        // Collage addition sequence: creating a release dir inside a collage.
        if vpath.collage.is_some() && vpath.release.is_some() {
            debug!("FUSE: setting {:?} as in-progress collage addition", spath);
            self.in_progress_collage_additions
                .insert(spath.to_string_lossy().to_string(), true);
            let inode = self.inodes.calc_inode(&spath);
            let attr = self.dir_attr(inode);
            reply.entry(&ENTRY_TTL, &attr, self.generation);
            return;
        }

        match self.rose.mkdir(&vpath) {
            Ok(()) => {
                self.reset_getattr_caches();
                let inode = self.inodes.calc_inode(&spath);
                let attr = self.dir_attr(inode);
                reply.entry(&ENTRY_TTL, &attr, self.generation);
            }
            Err(e) => {
                reply.error(e);
            }
        }
    }

    // -----------------------------------------------------------------------
    // rmdir
    // -----------------------------------------------------------------------
    fn rmdir(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        debug!("FUSE: rmdir(parent={}, name={:?})", parent, name);
        let spath = match self.inodes.get_path(parent, Some(name)) {
            Ok(p) => p,
            Err(_) => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let vpath = match VirtualPath::parse(&spath) {
            Ok(vp) => vp,
            Err(e) => {
                reply.error(e);
                return;
            }
        };
        debug!("FUSE: rmdir parsed path={:?} -> {:?}", spath, vpath);

        match self.rose.rmdir(&vpath) {
            Ok(()) => {
                self.reset_getattr_caches();
                self.inodes.remove_path(&spath);
                // Clean up collage addition state.
                self.in_progress_collage_additions
                    .remove(&spath.to_string_lossy().to_string());
                reply.ok();
            }
            Err(e) => {
                reply.error(e);
            }
        }
    }

    // -----------------------------------------------------------------------
    // rename
    // -----------------------------------------------------------------------
    fn rename(
        &mut self,
        _req: &Request<'_>,
        parent: u64,
        name: &OsStr,
        newparent: u64,
        newname: &OsStr,
        _flags: u32,
        reply: ReplyEmpty,
    ) {
        debug!(
            "FUSE: rename(parent={}, name={:?}, newparent={}, newname={:?})",
            parent, name, newparent, newname
        );
        let old_spath = match self.inodes.get_path(parent, Some(name)) {
            Ok(p) => p,
            Err(_) => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let new_spath = match self.inodes.get_path(newparent, Some(newname)) {
            Ok(p) => p,
            Err(_) => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let old_vpath = match VirtualPath::parse(&old_spath) {
            Ok(vp) => vp,
            Err(e) => {
                reply.error(e);
                return;
            }
        };
        let new_vpath = match VirtualPath::parse(&new_spath) {
            Ok(vp) => vp,
            Err(e) => {
                reply.error(e);
                return;
            }
        };

        match self.rose.rename(&old_vpath, &new_vpath) {
            Ok(()) => {
                self.reset_getattr_caches();
                self.inodes.rename_path(&old_spath, &new_spath);
                reply.ok();
            }
            Err(e) => {
                reply.error(e);
            }
        }
    }

    // =======================================================================
    // No-op stubs for tool compatibility
    // =======================================================================

    fn forget(&mut self, _req: &Request<'_>, _ino: u64, _nlookup: u64) {
        debug!("FUSE: forget(ino={})", _ino);
        self.reset_getattr_caches();
    }

    fn mknod(
        &mut self,
        _req: &Request<'_>,
        parent: u64,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
        _rdev: u32,
        reply: ReplyEntry,
    ) {
        debug!("FUSE: mknod(parent={}, name={:?})", parent, name);
        let path = match self.inodes.get_path(parent, Some(name)) {
            Ok(p) => p,
            Err(_) => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let inode = self.inodes.calc_inode(&path);
        let attr = self.file_attr(inode);
        reply.entry(&ENTRY_TTL, &attr, self.generation);
    }

    fn flush(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _fh: u64,
        _lock_owner: u64,
        reply: ReplyEmpty,
    ) {
        debug!("FUSE: flush(ino={}, fh={})", _ino, _fh);
        reply.ok();
    }

    fn setattr(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _mode: Option<u32>,
        _uid: Option<u32>,
        _gid: Option<u32>,
        _size: Option<u64>,
        _atime: Option<fuser::TimeOrNow>,
        _mtime: Option<fuser::TimeOrNow>,
        _ctime: Option<SystemTime>,
        fh: Option<u64>,
        _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>,
        _bkuptime: Option<SystemTime>,
        _flags: Option<u32>,
        reply: ReplyAttr,
    ) {
        debug!("FUSE: setattr(ino={}, fh={:?})", ino, fh);
        // Delegate to getattr — return whatever getattr returns.
        self.getattr(_req, ino, fh, reply);
    }

    fn getxattr(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _name: &OsStr,
        _size: u32,
        reply: ReplyXattr,
    ) {
        debug!("FUSE: getxattr(ino={}, name={:?})", _ino, _name);
        // ENODATA is the Linux equivalent of ENOATTR.
        #[cfg(target_os = "macos")]
        {
            reply.error(93); // ENOATTR on macOS
        }
        #[cfg(not(target_os = "macos"))]
        {
            reply.error(libc::ENODATA);
        }
    }

    fn setxattr(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _name: &OsStr,
        _value: &[u8],
        _flags: i32,
        _position: u32,
        reply: ReplyEmpty,
    ) {
        debug!("FUSE: setxattr(ino={})", _ino);
        reply.ok();
    }

    fn listxattr(&mut self, _req: &Request<'_>, _ino: u64, _size: u32, reply: ReplyXattr) {
        debug!("FUSE: listxattr(ino={})", _ino);
        // Reply with size=0 indicating empty xattr list.
        reply.size(0);
    }

    fn removexattr(&mut self, _req: &Request<'_>, _ino: u64, _name: &OsStr, reply: ReplyEmpty) {
        debug!("FUSE: removexattr(ino={}, name={:?})", _ino, _name);
        #[cfg(target_os = "macos")]
        {
            reply.error(93); // ENOATTR on macOS
        }
        #[cfg(not(target_os = "macos"))]
        {
            reply.error(libc::ENODATA);
        }
    }

    fn statfs(&mut self, _req: &Request<'_>, _ino: u64, reply: ReplyStatfs) {
        debug!("FUSE: statfs(ino={})", _ino);
        reply.statfs(
            1024 * 1024 * 16, // blocks: 16GB worth of 4KB blocks
            1024 * 1024 * 16, // bfree
            1024 * 1024 * 16, // bavail
            1024 * 128,       // files (total inodes)
            1024 * 64,        // ffree
            4096,             // bsize (block size)
            255,              // namelen
            4096,             // frsize (fragment size)
        );
    }

    fn access(&mut self, _req: &Request<'_>, _ino: u64, _mask: i32, reply: ReplyEmpty) {
        debug!("FUSE: access(ino={}, mask={})", _ino, _mask);
        reply.ok();
    }

    fn fsync(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _fh: u64,
        _datasync: bool,
        reply: ReplyEmpty,
    ) {
        debug!("FUSE: fsync(ino={}, fh={})", _ino, _fh);
        reply.ok();
    }

    fn fallocate(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _fh: u64,
        _offset: i64,
        _length: i64,
        _mode: i32,
        reply: ReplyEmpty,
    ) {
        debug!("FUSE: fallocate(ino={})", _ino);
        reply.ok();
    }

    fn fsyncdir(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _fh: u64,
        _datasync: bool,
        reply: ReplyEmpty,
    ) {
        debug!("FUSE: fsyncdir(ino={})", _ino);
        reply.ok();
    }
}

// ---------------------------------------------------------------------------
// mount / unmount
// ---------------------------------------------------------------------------

/// Mount the Rose virtual filesystem at the configured mount directory.
pub fn mount_virtualfs(config: &Config, debug: bool) -> Result<(), Box<dyn std::error::Error>> {
    let mount_dir = &config.vfs.mount_dir;
    info!("Mounting Rose VFS at {:?}", mount_dir);

    let fs = RoseFs::new(config.clone());

    let mut mount_options = vec![
        MountOption::FSName("rose".to_string()),
        MountOption::AutoUnmount,
        MountOption::AllowOther,
    ];

    if debug {
        // fuser doesn't have a direct "debug" option, but we can add custom options.
        mount_options.push(MountOption::CUSTOM("debug".to_string()));
    }

    #[cfg(target_os = "macos")]
    {
        mount_options.push(MountOption::CUSTOM("noappledouble".to_string()));
        mount_options.push(MountOption::CUSTOM("nolocalcaches".to_string()));
    }

    fuser::mount2(fs, mount_dir, &mount_options)?;
    info!("Filesystem unmounted cleanly.");
    Ok(())
}

/// Unmount the Rose virtual filesystem.
pub fn unmount_virtualfs(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let mount_dir = &config.vfs.mount_dir;
    info!("Unmounting Rose VFS at {:?}", mount_dir);
    let status = Command::new("umount")
        .arg(mount_dir.to_string_lossy().as_ref())
        .status()?;
    if !status.success() {
        warn!("umount exited with status: {}", status);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    info!("Rose VFS v{}", rose_core::VERSION);
    info!("Mounting at {:?}", cli.mount);

    let config = match Config::parse(cli.config.as_deref()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading configuration: {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = mount_virtualfs(&config, false) {
        eprintln!("Error mounting filesystem: {}", e);
        std::process::exit(1);
    }
}
