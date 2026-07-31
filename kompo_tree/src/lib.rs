//! Path index and file table for the embedded filesystem.
//!
//! This is a candidate replacement for the trie in `kompo_storage`, built
//! around a flat node arena instead of a LOUDS trie. It exposes the same
//! operations, with two deliberate differences:
//!
//! * paths are taken as raw bytes rather than pre-split components, because
//!   every caller in `kompo_fs` already has an absolute path in hand and the
//!   splitting was pure overhead;
//! * a directory knows its entries as a contiguous range, so `readdir` neither
//!   searches nor allocates.
//!
//! Inode numbers are node ids offset by [`INO_BASE`]. That makes them unique by
//! construction rather than by hash luck, and keeps them clear of the range a
//! real filesystem hands out -- the VFS is mixed with the real one, and while
//! `st_dev` is what actually separates the two, `struct dirent` carries a bare
//! `d_ino` with no device field to qualify it.

mod tree;

pub use tree::{NodeId, ROOT, Tree};

use std::sync::RwLock;

/// Fake device number, shared with `kompo_storage` so both report the same
/// `st_dev`. Major 2222 is outside the range Linux hands out, and outside the
/// anonymous major 0 that tmpfs and overlayfs use.
pub const DEV: libc::dev_t = libc::makedev(2222, 0);

/// Inode numbers are `INO_BASE | node_id`.
///
/// Real filesystems allocate inode numbers from the bottom of the range: ext4
/// caps at 32 bits, and XFS and btrfs stay far below this bit for any
/// achievable filesystem size. Setting bit 62 keeps virtual inodes out of the
/// way while the node id below it stays dense and collision-free.
pub const INO_BASE: u64 = 1 << 62;

/// Inode number reported for a node.
#[inline]
pub const fn inode(id: NodeId) -> u64 {
    INO_BASE | id as u64
}

#[derive(Clone, Copy, Debug)]
struct OpenFile {
    node: NodeId,
    offset: u64,
    is_dir: bool,
}

/// Open file descriptors, indexed directly by fd.
///
/// The fds come from `dup(0)`, so they are real, unique, and dense -- a plain
/// vector beats a hash map here.
#[derive(Default)]
struct FdTable {
    slots: Vec<Option<OpenFile>>,
}

impl FdTable {
    fn insert(&mut self, fd: i32, open: OpenFile) {
        if fd < 0 {
            return;
        }
        let i = fd as usize;
        if i >= self.slots.len() {
            self.slots.resize(i + 1, None);
        }
        self.slots[i] = Some(open);
    }

    fn get(&self, fd: i32) -> Option<&OpenFile> {
        if fd < 0 {
            return None;
        }
        self.slots.get(fd as usize)?.as_ref()
    }

    fn get_mut(&mut self, fd: i32) -> Option<&mut OpenFile> {
        if fd < 0 {
            return None;
        }
        self.slots.get_mut(fd as usize)?.as_mut()
    }

    fn remove(&mut self, fd: i32) -> Option<OpenFile> {
        if fd < 0 {
            return None;
        }
        self.slots.get_mut(fd as usize)?.take()
    }

    fn open_fds(&self) -> impl Iterator<Item = i32> + '_ {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, slot)| slot.as_ref().map(|_| i as i32))
    }
}

/// An open directory handle.
///
/// The `dirent` lives here rather than being allocated per call, which matches
/// what POSIX promises callers (`readdir` returns storage owned by the `DIR`)
/// and means nothing has to be freed.
pub struct FsDir {
    pub fd: i32,
    node: NodeId,
    offset: u32,
    entry: libc::dirent,
}

impl FsDir {
    fn new(fd: i32, node: NodeId) -> Self {
        Self {
            fd,
            node,
            offset: 0,
            entry: unsafe { std::mem::zeroed() },
        }
    }
}

impl std::fmt::Debug for FsDir {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FsDir")
            .field("fd", &self.fd)
            .field("node", &self.node)
            .field("offset", &self.offset)
            .finish_non_exhaustive()
    }
}

pub struct Fs<'a> {
    tree: Tree<'a>,
    fds: RwLock<FdTable>,
    uid: libc::uid_t,
    gid: libc::gid_t,
}

impl<'a> Fs<'a> {
    /// Build from the embedded blobs.
    ///
    /// `paths` is the NUL separated `PATHS` blob, `files` is `FILES` (or the
    /// decompressed buffer), and `file_offsets` is `FILES_SIZES`: one more
    /// entry than there are paths.
    pub fn new(paths: &'a [u8], files: &'a [u8], file_offsets: &'a [u64]) -> Self {
        Self {
            tree: Tree::build(paths, files, file_offsets),
            fds: RwLock::new(FdTable::default()),
            // Constant for the process, and a syscall each on some platforms,
            // so it is read once instead of on every stat.
            uid: unsafe { libc::getuid() },
            gid: unsafe { libc::getgid() },
        }
    }

    pub fn tree(&self) -> &Tree<'a> {
        &self.tree
    }

    // -- lookup -----------------------------------------------------------

    /// Resolve an absolute path to a node.
    #[inline]
    pub fn lookup(&self, path: &[u8]) -> Option<NodeId> {
        self.tree.lookup(path)
    }

    pub fn is_dir_exists_from_path(&self, path: &[u8]) -> bool {
        self.tree
            .lookup(path)
            .is_some_and(|id| self.tree.is_dir(id))
    }

    // -- metadata ---------------------------------------------------------

    fn fill_stat(&self, id: NodeId, st: &mut libc::stat) -> Option<()> {
        if !self.tree.exists(id) {
            return None;
        }
        // Zero first: unset and padding fields then read as 0 rather than
        // whatever the caller's buffer happened to hold.
        *st = unsafe { std::mem::zeroed() };
        st.st_dev = DEV;
        st.st_ino = inode(id);
        st.st_nlink = 1;
        st.st_uid = self.uid;
        st.st_gid = self.gid;
        st.st_blksize = 4096;

        if self.tree.is_dir(id) {
            st.st_mode = libc::S_IFDIR | 0o555;
            st.st_size = 1;
        } else {
            let len = self.tree.data(id).map_or(0, <[u8]>::len);
            st.st_mode = libc::S_IFREG | 0o444;
            st.st_size = len as _;
            st.st_blocks = (len.div_ceil(512).div_ceil(8) * 8) as _;
        }
        Some(())
    }

    pub fn stat(&self, path: &[u8], st: &mut libc::stat) -> Option<i32> {
        self.fill_stat(self.tree.lookup(path)?, st)?;
        Some(0)
    }

    /// The image holds no symlinks, so this is `stat`.
    pub fn lstat(&self, path: &[u8], st: &mut libc::stat) -> Option<i32> {
        self.stat(path, st)
    }

    pub fn fstat(&self, fd: i32, st: &mut libc::stat) -> Option<i32> {
        let node = self.fds.read().ok()?.get(fd)?.node;
        self.fill_stat(node, st)?;
        Some(0)
    }

    // -- files ------------------------------------------------------------

    fn open_node(&self, id: NodeId) -> Option<i32> {
        // A real fd, so it can never collide with one the process already has.
        let fd = unsafe { libc::dup(0) };
        if fd < 0 {
            return None;
        }
        let Ok(mut fds) = self.fds.write() else {
            unsafe { libc::close(fd) };
            return None;
        };
        fds.insert(
            fd,
            OpenFile {
                node: id,
                offset: 0,
                is_dir: self.tree.is_dir(id),
            },
        );
        Some(fd)
    }

    pub fn open(&self, path: &[u8]) -> Option<i32> {
        self.open_node(self.tree.lookup(path)?)
    }

    /// Same as [`Fs::open`]; the caller resolves `dirfd` before calling.
    pub fn open_at(&self, path: &[u8]) -> Option<i32> {
        self.open(path)
    }

    pub fn read(&self, fd: i32, buf: &mut [u8]) -> Option<isize> {
        let mut fds = self.fds.write().ok()?;
        let open = fds.get_mut(fd)?;
        if open.is_dir {
            return None;
        }
        let data = self.tree.data(open.node)?;
        let offset = open.offset as usize;
        if offset >= data.len() {
            return Some(0);
        }
        let n = (data.len() - offset).min(buf.len());
        buf[..n].copy_from_slice(&data[offset..offset + n]);
        open.offset += n as u64;
        Some(n as isize)
    }

    pub fn close(&self, fd: i32) -> i32 {
        if let Ok(mut fds) = self.fds.write() {
            fds.remove(fd);
        }
        0
    }

    /// Pointer to a file body, for callers that map it rather than read it.
    ///
    /// Unlike `kompo_storage::Fs::file_read` this reports a missing path as
    /// `None` instead of panicking.
    pub fn file_read(&self, path: &[u8]) -> Option<*const u8> {
        self.tree.data(self.tree.lookup(path)?).map(<[u8]>::as_ptr)
    }

    pub fn is_fd_exists(&self, fd: i32) -> bool {
        self.fds.read().is_ok_and(|fds| fds.get(fd).is_some())
    }

    // -- directories ------------------------------------------------------

    pub fn opendir(&self, path: &[u8]) -> Option<FsDir> {
        let id = self.tree.lookup(path)?;
        if !self.tree.is_dir(id) {
            return None;
        }
        Some(FsDir::new(self.open_node(id)?, id))
    }

    pub fn fdopendir(&self, fd: i32) -> Option<FsDir> {
        let open = *self.fds.read().ok()?.get(fd)?;
        if !open.is_dir {
            return None;
        }
        Some(FsDir::new(fd, open.node))
    }

    /// Next entry, or a null pointer at end of directory.
    ///
    /// The returned pointer borrows `dir` and stays valid until the next call,
    /// exactly like `readdir(3)`. Callers must not free it.
    pub fn readdir(&self, dir: &mut FsDir) -> Option<*mut libc::dirent> {
        if !self.is_fd_exists(dir.fd) {
            return None;
        }
        let Some(child) = self.tree.child_at(dir.node, dir.offset) else {
            return Some(std::ptr::null_mut());
        };
        dir.offset += 1;
        let next = u64::from(dir.offset);
        self.fill_dirent(child, next, &mut dir.entry);
        Some(&raw mut dir.entry)
    }

    fn fill_dirent(&self, id: NodeId, next_offset: u64, out: &mut libc::dirent) {
        *out = unsafe { std::mem::zeroed() };
        out.d_ino = inode(id);
        out.d_type = if self.tree.is_dir(id) {
            libc::DT_DIR
        } else {
            libc::DT_REG
        };
        out.d_reclen = size_of::<libc::dirent>() as _;

        let name = self.tree.name(id);
        // Leave room for the terminator; the buffer is already zeroed.
        let n = name.len().min(out.d_name.len() - 1);
        for (slot, &byte) in out.d_name.iter_mut().zip(&name[..n]) {
            *slot = byte as _;
        }

        #[cfg(target_os = "linux")]
        {
            out.d_off = next_offset as _;
        }
        #[cfg(target_os = "macos")]
        {
            out.d_seekoff = next_offset;
            out.d_namlen = n as _;
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        let _ = next_offset;
    }

    pub fn closedir(&self, dir: &FsDir) -> i32 {
        self.close(dir.fd)
    }

    pub fn rewinddir(&self, dir: &mut FsDir) {
        dir.offset = 0;
    }

    pub fn is_dir_exists(&self, dir: &FsDir) -> bool {
        self.is_fd_exists(dir.fd)
    }

    #[cfg(target_os = "macos")]
    pub fn getattrlist(
        &self,
        path: &[u8],
        attr_list: &libc::attrlist,
        attr_buf: *mut libc::c_void,
        attr_buf_size: usize,
    ) -> Option<i32> {
        #[repr(C)]
        struct AttrBufNameAndObjType {
            length: u32,
            name_ref: libc::attrreference_t,
            obj_type: u32,
        }

        if attr_list.commonattr != (libc::ATTR_CMN_NAME | libc::ATTR_CMN_OBJTYPE) {
            return None;
        }

        let id = self.tree.lookup(path)?;
        let obj_type: u32 = if self.tree.is_dir(id) { 2 } else { 1 }; // VDIR : VREG
        let basename = self.tree.name(id);
        let basename_len = basename.len() + 1;

        let header_size = size_of::<AttrBufNameAndObjType>();
        let total_size = header_size + basename_len;
        if total_size > attr_buf_size {
            return None;
        }

        let result = AttrBufNameAndObjType {
            length: total_size as u32,
            name_ref: libc::attrreference_t {
                attr_dataoffset: (header_size
                    - std::mem::offset_of!(AttrBufNameAndObjType, name_ref))
                    as i32,
                attr_length: basename_len as u32,
            },
            obj_type,
        };

        unsafe {
            std::ptr::write(attr_buf as *mut AttrBufNameAndObjType, result);
            let name_ptr = (attr_buf as *mut u8).add(header_size);
            std::ptr::copy_nonoverlapping(basename.as_ptr(), name_ptr, basename.len());
            *name_ptr.add(basename.len()) = 0;
        }

        Some(0)
    }
}

impl Drop for Fs<'_> {
    fn drop(&mut self) {
        if let Ok(fds) = self.fds.read() {
            for fd in fds.open_fds() {
                unsafe { libc::close(fd) };
            }
        }
    }
}

/// Assembles the blobs an [`Fs`] is built from, for tests and benchmarks.
///
/// The real filesystem is built from static data linked into the binary; this
/// leaks its buffers to reach the same `'static` lifetime.
pub struct FsBuilder {
    paths: Vec<u8>,
    files: Vec<u8>,
    offsets: Vec<u64>,
}

impl Default for FsBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl FsBuilder {
    pub fn new() -> Self {
        Self {
            paths: Vec::new(),
            files: Vec::new(),
            offsets: vec![0],
        }
    }

    pub fn push(&mut self, path: impl AsRef<[u8]>, data: impl AsRef<[u8]>) -> &mut Self {
        self.paths.extend_from_slice(path.as_ref());
        self.paths.push(0);
        self.files.extend_from_slice(data.as_ref());
        self.offsets.push(self.files.len() as u64);
        self
    }

    /// Leak the blobs and build. Intended for tests and benchmarks.
    pub fn leak(self) -> Fs<'static> {
        Fs::new(
            Vec::leak(self.paths),
            Vec::leak(self.files),
            Vec::leak(self.offsets),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fs() -> Fs<'static> {
        let mut b = FsBuilder::new();
        b.push("/usr/bin/ls", b"ls_content");
        b.push("/usr/bin/cat", b"cat_content_here");
        b.push("/usr/bin/hoge/fuga", b"hoge_fuga_content");
        b.push("/usr/bin/fuga", b"fuga_content");
        b.push("/usr/empty", b"");
        b.leak()
    }

    fn zeroed_stat() -> libc::stat {
        unsafe { std::mem::zeroed() }
    }

    fn drain(fs: &Fs, dir: &mut FsDir) -> Vec<String> {
        let mut out = Vec::new();
        loop {
            let entry = fs.readdir(dir).expect("readdir on a live handle");
            if entry.is_null() {
                break;
            }
            let name: Vec<u8> = unsafe { &*entry }
                .d_name
                .iter()
                .take_while(|&&c| c != 0)
                .map(|&c| c as u8)
                .collect();
            out.push(String::from_utf8_lossy(&name).into_owned());
        }
        out
    }

    #[test]
    fn open_and_read_a_file() {
        let fs = fs();
        let fd = fs.open(b"/usr/bin/ls").unwrap();
        let mut buf = [0u8; 128];
        assert_eq!(fs.read(fd, &mut buf), Some(10));
        assert_eq!(&buf[..10], b"ls_content");
        // Second read is EOF.
        assert_eq!(fs.read(fd, &mut buf), Some(0));
        assert_eq!(fs.close(fd), 0);
        assert!(!fs.is_fd_exists(fd));
        unsafe { libc::close(fd) };
    }

    #[test]
    fn read_in_chunks_advances_the_offset() {
        let fs = fs();
        let fd = fs.open(b"/usr/bin/cat").unwrap();
        let mut buf = [0u8; 4];
        assert_eq!(fs.read(fd, &mut buf), Some(4));
        assert_eq!(&buf, b"cat_");
        assert_eq!(fs.read(fd, &mut buf), Some(4));
        assert_eq!(&buf, b"cont");
        fs.close(fd);
        unsafe { libc::close(fd) };
    }

    #[test]
    fn empty_file_reads_zero_bytes() {
        let fs = fs();
        let fd = fs.open(b"/usr/empty").unwrap();
        let mut buf = [0u8; 8];
        assert_eq!(fs.read(fd, &mut buf), Some(0));
        fs.close(fd);
        unsafe { libc::close(fd) };
    }

    #[test]
    fn open_and_read_reject_bad_input() {
        let fs = fs();
        assert_eq!(fs.open(b"/usr/bin/nonexistent"), None);
        assert_eq!(fs.read(9999, &mut [0u8; 8]), None);

        // A directory opens, but does not read.
        let fd = fs.open(b"/usr/bin").unwrap();
        assert_eq!(fs.read(fd, &mut [0u8; 8]), None);
        fs.close(fd);
        unsafe { libc::close(fd) };
    }

    #[test]
    fn concurrent_opens_get_independent_offsets() {
        let fs = fs();
        let a = fs.open(b"/usr/bin/ls").unwrap();
        let b = fs.open(b"/usr/bin/ls").unwrap();
        assert_ne!(a, b);

        let mut buf = [0u8; 2];
        fs.read(a, &mut buf).unwrap();
        assert_eq!(&buf, b"ls");
        // `b` is untouched and still at the start.
        fs.read(b, &mut buf).unwrap();
        assert_eq!(&buf, b"ls");

        fs.close(a);
        assert!(!fs.is_fd_exists(a));
        assert!(fs.is_fd_exists(b));
        fs.close(b);
        unsafe {
            libc::close(a);
            libc::close(b);
        }
    }

    #[test]
    fn stat_reports_files_and_directories() {
        let fs = fs();
        let mut st = zeroed_stat();

        assert_eq!(fs.stat(b"/usr/bin/ls", &mut st), Some(0));
        assert_eq!(st.st_size, 10);
        assert_eq!(st.st_mode & libc::S_IFMT, libc::S_IFREG);
        assert_eq!(st.st_dev, DEV);

        assert_eq!(fs.stat(b"/usr/bin", &mut st), Some(0));
        assert_eq!(st.st_mode & libc::S_IFMT, libc::S_IFDIR);
        assert_eq!(st.st_dev, DEV);

        assert_eq!(fs.stat(b"/nonexistent", &mut st), None);
        assert_eq!(fs.lstat(b"/usr/bin/ls", &mut st), Some(0));
    }

    #[test]
    fn fstat_matches_stat() {
        let fs = fs();
        let mut by_path = zeroed_stat();
        fs.stat(b"/usr/bin/cat", &mut by_path).unwrap();

        let fd = fs.open(b"/usr/bin/cat").unwrap();
        let mut by_fd = zeroed_stat();
        assert_eq!(fs.fstat(fd, &mut by_fd), Some(0));

        assert_eq!(by_fd.st_ino, by_path.st_ino);
        assert_eq!(by_fd.st_size, 16);
        assert_eq!(fs.fstat(9999, &mut by_fd), None);

        fs.close(fd);
        unsafe { libc::close(fd) };
    }

    #[test]
    fn stat_overwrites_stale_buffer_contents() {
        let fs = fs();
        let mut st = zeroed_stat();
        st.st_size = 12345;
        st.st_mtime = 999;
        fs.stat(b"/usr/empty", &mut st).unwrap();
        assert_eq!(st.st_size, 0);
        assert_eq!(st.st_mtime, 0);
    }

    #[test]
    fn inodes_are_unique_and_out_of_the_real_range() {
        let fs = fs();
        let paths: [&[u8]; 7] = [
            b"/",
            b"/usr",
            b"/usr/bin",
            b"/usr/bin/ls",
            b"/usr/bin/cat",
            b"/usr/bin/hoge",
            b"/usr/bin/hoge/fuga",
        ];
        let mut seen = std::collections::HashSet::new();
        for p in paths {
            let mut st = zeroed_stat();
            fs.stat(p, &mut st).unwrap();
            assert!(st.st_ino >= INO_BASE, "{:?} got {:#x}", p, st.st_ino);
            assert!(seen.insert(st.st_ino), "duplicate inode for {:?}", p);
        }
    }

    #[test]
    fn the_same_file_keeps_one_identity_across_spellings() {
        let fs = fs();
        let mut direct = zeroed_stat();
        let mut indirect = zeroed_stat();
        fs.stat(b"/usr/bin/ls", &mut direct).unwrap();
        fs.stat(b"/usr/bin/hoge/../ls", &mut indirect).unwrap();
        assert_eq!(direct.st_ino, indirect.st_ino);
        assert_eq!(direct.st_dev, indirect.st_dev);
    }

    #[test]
    fn opendir_lists_entries_once() {
        let fs = fs();
        let mut dir = fs.opendir(b"/usr/bin").unwrap();
        let entries = drain(&fs, &mut dir);
        assert_eq!(entries, vec!["cat", "fuga", "hoge", "ls"]);

        // Past the end stays at the end.
        assert!(fs.readdir(&mut dir).unwrap().is_null());

        let fd = dir.fd;
        assert_eq!(fs.closedir(&dir), 0);
        assert!(!fs.is_fd_exists(fd));
        unsafe { libc::close(fd) };
    }

    #[test]
    fn readdir_reports_entry_types_and_inodes() {
        let fs = fs();
        let mut dir = fs.opendir(b"/usr/bin").unwrap();
        let mut seen = Vec::new();
        loop {
            let e = fs.readdir(&mut dir).unwrap();
            if e.is_null() {
                break;
            }
            let e = unsafe { &*e };
            let name: Vec<u8> = e
                .d_name
                .iter()
                .take_while(|&&c| c != 0)
                .map(|&c| c as u8)
                .collect();
            seen.push((
                String::from_utf8_lossy(&name).into_owned(),
                e.d_type,
                e.d_ino,
            ));
        }

        let hoge = seen.iter().find(|(n, ..)| n == "hoge").unwrap();
        assert_eq!(hoge.1, libc::DT_DIR);
        let ls = seen.iter().find(|(n, ..)| n == "ls").unwrap();
        assert_eq!(ls.1, libc::DT_REG);

        // d_ino must agree with what stat reports for the same entry.
        let mut st = zeroed_stat();
        fs.stat(b"/usr/bin/ls", &mut st).unwrap();
        assert_eq!(ls.2, st.st_ino);

        let fd = dir.fd;
        fs.closedir(&dir);
        unsafe { libc::close(fd) };
    }

    #[test]
    fn rewinddir_restarts_the_listing() {
        let fs = fs();
        let mut dir = fs.opendir(b"/usr/bin").unwrap();
        let first = drain(&fs, &mut dir);
        fs.rewinddir(&mut dir);
        let second = drain(&fs, &mut dir);
        assert_eq!(first, second);

        let fd = dir.fd;
        fs.closedir(&dir);
        unsafe { libc::close(fd) };
    }

    #[test]
    fn opendir_rejects_files_and_missing_paths() {
        let fs = fs();
        assert!(fs.opendir(b"/usr/bin/ls").is_none());
        assert!(fs.opendir(b"/nonexistent").is_none());
        assert!(fs.is_dir_exists_from_path(b"/usr/bin"));
        assert!(!fs.is_dir_exists_from_path(b"/usr/bin/ls"));
        assert!(!fs.is_dir_exists_from_path(b"/nonexistent"));
    }

    #[test]
    fn fdopendir_accepts_only_directory_fds() {
        let fs = fs();
        let dir_fd = fs.open(b"/usr/bin").unwrap();
        let dir = fs.fdopendir(dir_fd).unwrap();
        assert_eq!(dir.fd, dir_fd);
        assert!(fs.is_dir_exists(&dir));

        let file_fd = fs.open(b"/usr/bin/ls").unwrap();
        assert!(fs.fdopendir(file_fd).is_none());

        fs.close(dir_fd);
        fs.close(file_fd);
        unsafe {
            libc::close(dir_fd);
            libc::close(file_fd);
        }
    }

    #[test]
    fn readdir_on_a_closed_handle_reports_failure() {
        let fs = fs();
        let mut dir = fs.opendir(b"/usr/bin").unwrap();
        let fd = dir.fd;
        fs.closedir(&dir);
        assert!(fs.readdir(&mut dir).is_none());
        unsafe { libc::close(fd) };
    }

    #[test]
    fn file_read_returns_the_body() {
        let fs = fs();
        let ptr = fs.file_read(b"/usr/bin/ls").unwrap();
        let body = unsafe { std::slice::from_raw_parts(ptr, 10) };
        assert_eq!(body, b"ls_content");
        assert!(fs.file_read(b"/nonexistent").is_none());
        // A directory has no body.
        assert!(fs.file_read(b"/usr/bin").is_none());
    }

    #[test]
    fn shared_across_threads() {
        let fs = std::sync::Arc::new(fs());
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let fs = std::sync::Arc::clone(&fs);
                std::thread::spawn(move || {
                    for _ in 0..200 {
                        let mut st = zeroed_stat();
                        assert_eq!(fs.stat(b"/usr/bin/ls", &mut st), Some(0));
                        let fd = fs.open(b"/usr/bin/cat").unwrap();
                        let mut buf = [0u8; 16];
                        assert_eq!(fs.read(fd, &mut buf), Some(16));
                        fs.close(fd);
                        unsafe { libc::close(fd) };
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
    }
}
