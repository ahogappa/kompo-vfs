use fxhash::FxHasher;
use std::collections::HashMap;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::hash::Hash;
use std::hash::Hasher;
use std::os::unix::ffi::OsStrExt;
use trie_rs::map::Trie;
use trie_rs::map::TrieBuilder;

#[derive(Debug, PartialEq)]
enum FileType<'a> {
    File {
        file: &'a [u8],
        offset: u64,
        inode: u64,
    },
    Directory {
        inode: u64,
        entries: Vec<Vec<OsString>>,
    },
}

#[derive(Debug)]
pub struct FsDir {
    pub fd: i32,
    offset: u64,
}

#[derive(Debug)]
pub struct Fs<'a> {
    trie: Trie<&'a OsStr, &'a [u8]>,
    fd_map: HashMap<i32, FileType<'a>>,
}

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
type DirEntryName = [u8; 256];
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
fn convert_byte(b: &u8) -> u8 {
    *b
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
type DirEntryName = [i8; 256];
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn convert_byte(b: u8) -> i8 {
    b as i8
}

#[cfg(target_os = "macos")]
type DirEntryName = [i8; 1024];
#[cfg(target_os = "macos")]
fn convert_byte(b: &u8) -> i8 {
    *b as i8
}

impl<'a> Fs<'a> {
    const DEV: libc::dev_t = libc::makedev(2222, 0); // create fake device number. TODO: get unused device number dynamically.

    pub fn new(builder: TrieBuilder<&'static OsStr, &'static [u8]>) -> Self {
        Self {
            trie: builder.build(),
            fd_map: HashMap::new(),
        }
    }

    pub fn entries(&self) {
        let hoge: Vec<(OsString, &&[u8])> = self.trie.iter().collect();
        dbg!(hoge);
    }

    fn get_inode_from_path(&self, path: &Vec<&OsStr>) -> u64 {
        let mut hasher = FxHasher::default();
        path.hash(&mut hasher);

        hasher.finish()
    }

    fn get_file_type_from_path(&self, search_path: &Vec<&OsStr>) -> Option<FileType<'a>> {
        if let Some(file) = self.trie.exact_match(search_path) {
            let inode = self.get_inode_from_path(search_path);

            return Some(FileType::File {
                file,
                offset: 0,
                inode,
            });
        }

        let depth = search_path.len() + 1;
        let mut uniq_file = HashSet::new();

        let entries: Vec<_> = self
            .trie
            .predictive_search(search_path)
            .filter_map(|(path, _): (Vec<&OsStr>, _)| {
                if path.len() >= depth {
                    let id = self.get_inode_from_path(&path);

                    if uniq_file.contains(&id) {
                        None
                    } else {
                        uniq_file.insert(id);
                        let next_depth_path = path
                            .iter()
                            .take(depth)
                            .map(|&s| s.to_os_string())
                            .collect::<Vec<OsString>>();
                        Some(next_depth_path)
                    }
                } else {
                    None
                }
            })
            .collect::<Vec<Vec<OsString>>>();

        if !entries.is_empty() {
            // dbg!(&search_path);
            let inode = self.get_inode_from_path(search_path);

            return Some(FileType::Directory { inode, entries });
        }

        None
    }

    pub fn is_fd_exists(&self, fd: i32) -> bool {
        self.fd_map.contains_key(&fd)
    }

    pub fn is_dir_exists(&self, dir: &FsDir) -> bool {
        self.is_fd_exists(dir.fd)
    }

    pub fn is_dir_exists_from_path(&self, path: &Vec<&OsStr>) -> bool {
        matches!(
            self.get_file_type_from_path(path),
            Some(FileType::Directory { .. })
        )
    }

    fn get_stat_from_file_type(&self, file_type: &FileType) -> libc::stat {
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        let stat_ptr = stat.as_mut_ptr();

        unsafe {
            match file_type {
                FileType::File { file, inode, .. } => {
                    (*stat_ptr).st_dev = Self::DEV;
                    (*stat_ptr).st_ino = *inode;
                    (*stat_ptr).st_mode = libc::S_IFREG // 444
                                    | libc::S_IRUSR
                                    | libc::S_IRGRP
                                    | libc::S_IROTH;
                    (*stat_ptr).st_nlink = 1;
                    (*stat_ptr).st_uid = libc::getuid();
                    (*stat_ptr).st_gid = libc::getgid();
                    (*stat_ptr).st_rdev = 0;
                    (*stat_ptr).st_size = file.len() as _;
                    (*stat_ptr).st_blksize = 4096;
                    (*stat_ptr).st_blocks = (file.len().div_ceil(512).div_ceil(8) * 8) as i64;
                    (*stat_ptr).st_atime = 0;
                    (*stat_ptr).st_atime_nsec = 0;
                    (*stat_ptr).st_mtime = 0;
                    (*stat_ptr).st_mtime_nsec = 0;
                    (*stat_ptr).st_ctime = 0;
                    (*stat_ptr).st_ctime_nsec = 0;

                    stat.assume_init()
                }
                FileType::Directory { inode, .. } => {
                    (*stat_ptr).st_dev = Self::DEV;
                    (*stat_ptr).st_ino = *inode;
                    (*stat_ptr).st_mode = libc::S_IFDIR // 555
                                    | libc::S_IXUSR
                                    | libc::S_IRUSR
                                    | libc::S_IXGRP
                                    | libc::S_IRGRP
                                    | libc::S_IXOTH
                                    | libc::S_IROTH;
                    (*stat_ptr).st_nlink = 1;
                    (*stat_ptr).st_uid = libc::getuid();
                    (*stat_ptr).st_gid = libc::getgid();
                    (*stat_ptr).st_rdev = 0;
                    (*stat_ptr).st_size = 1;
                    (*stat_ptr).st_blksize = 4096;
                    (*stat_ptr).st_blocks = 0;
                    (*stat_ptr).st_atime = 0;
                    (*stat_ptr).st_atime_nsec = 0;
                    (*stat_ptr).st_mtime = 0;
                    (*stat_ptr).st_mtime_nsec = 0;
                    (*stat_ptr).st_ctime = 0;
                    (*stat_ptr).st_ctime_nsec = 0;

                    stat.assume_init()
                }
            }
        }
    }

    pub fn open(&mut self, path: &Vec<&OsStr>) -> Option<i32> {
        match self.get_file_type_from_path(path) {
            Some(file_type) => {
                let fd = unsafe { libc::dup(0) };

                self.fd_map.insert(fd, file_type);

                Some(fd)
            }
            None => None,
        }
    }

    pub fn open_at(&mut self, path: &Vec<&OsStr>) -> Option<i32> {
        match self.get_file_type_from_path(path) {
            Some(file_type) => {
                let fd = unsafe { libc::dup(0) };

                self.fd_map.insert(fd, file_type);

                Some(fd)
            }
            None => None,
        }
    }

    pub fn read(&mut self, fd: i32, buf: &mut [u8]) -> Option<isize> {
        match self.fd_map.get_mut(&fd) {
            Some(file_type) => match file_type {
                FileType::File { file, offset, .. } => {
                    if *offset == file.len() as u64 {
                        return Some(0);
                    }

                    let read_size = (file.len() - *offset as usize).min(buf.len());
                    buf[..read_size]
                        .copy_from_slice(&file[*offset as usize..*offset as usize + read_size]);

                    *offset += read_size as u64;

                    Some(read_size as isize)
                }
                FileType::Directory { .. } => todo!(),
            },
            None => None,
        }
    }

    pub fn close(&mut self, fd: i32) -> i32 {
        self.fd_map.remove(&fd);

        0
    }

    pub fn stat(&self, path: &Vec<&OsStr>, stat_buf: &mut libc::stat) -> Option<i32> {
        match self.get_file_type_from_path(path) {
            Some(file_type) => {
                *stat_buf = self.get_stat_from_file_type(&file_type);
                Some(0)
            }
            None => None,
        }
    }

    pub fn lstat(&self, path: &Vec<&OsStr>, stat_buf: &mut libc::stat) -> Option<i32> {
        self.stat(path, stat_buf)
    }

    pub fn fstat(&self, fd: i32, stat_buf: &mut libc::stat) -> Option<i32> {
        match self.fd_map.get(&fd) {
            Some(file_type) => {
                *stat_buf = self.get_stat_from_file_type(file_type);
                Some(0)
            }
            None => None,
        }
    }

    pub fn file_read(&self, path: &Vec<&OsStr>) -> Option<*const u8> {
        let file_type = self
            .get_file_type_from_path(path)
            .unwrap_or_else(|| panic!("not found path: {:?}", path));

        match file_type {
            FileType::File { file, .. } => Some(file.as_ptr()),
            _ => None,
        }
    }

    pub fn fdopendir(&self, fd: i32) -> Option<FsDir> {
        match self.fd_map.get(&fd) {
            Some(FileType::Directory { .. }) => Some(FsDir { fd, offset: 0 }),
            _ => None,
        }
    }

    pub fn readdir(&self, dir: &mut FsDir) -> Option<*mut libc::dirent> {
        match self.fd_map.get(&dir.fd) {
            Some(FileType::Directory { entries, .. }) => {
                if dir.offset >= entries.len() as u64 {
                    return Some(std::ptr::null_mut());
                }
                let full_path = &entries[dir.offset as usize];
                let full_path = full_path
                    .iter()
                    .map(|s| s.as_os_str())
                    .collect::<Vec<&OsStr>>();

                let file_type = match self.get_file_type_from_path(&full_path) {
                    Some(t) => match t {
                        FileType::File { .. } => libc::DT_REG,
                        FileType::Directory { .. } => libc::DT_DIR,
                    },
                    None => unreachable!(),
                };
                let inode = self.get_inode_from_path(&full_path);
                let dirent = Self::create_dirent(inode, file_type, full_path);

                dir.offset += 1;

                let dirent = Box::new(dirent);
                Some(Box::into_raw(dirent))
            }
            _ => None,
        }
    }

    #[cfg(target_os = "linux")]
    fn create_dirent(inode: u64, file_type: u8, full_path: Vec<&OsStr>) -> libc::dirent {
        let mut buf: DirEntryName = [0; 256];
        let last_path = full_path.last().unwrap();
        let convert_path: Vec<_> = last_path
            .as_bytes()
            .iter()
            .map(|b| convert_byte(*b))
            .collect();
        buf[..last_path.len()].copy_from_slice(&convert_path);

        libc::dirent {
            d_ino: inode,
            d_off: 0,    // TODO
            d_reclen: 0, // TODO
            d_type: file_type,
            d_name: buf,
        }
    }

    #[cfg(target_os = "macos")]
    fn create_dirent(inode: u64, file_type: u8, full_path: Vec<&OsStr>) -> libc::dirent {
        let mut buf: DirEntryName = [0; 1024];
        let last_path = full_path.last().unwrap();
        let convert_path: Vec<i8> = last_path.as_bytes().iter().map(convert_byte).collect();
        buf[..last_path.len()].copy_from_slice(&convert_path);

        libc::dirent {
            d_ino: inode,
            d_reclen: 0, // TODO
            d_type: file_type,
            d_name: buf,
            d_seekoff: 0, // TODO
            d_namlen: last_path.len() as u16,
        }
    }

    pub fn closedir(&mut self, dir: &FsDir) -> i32 {
        self.close(dir.fd)
    }

    pub fn opendir(&mut self, path: &Vec<&OsStr>) -> Option<FsDir> {
        match self.get_file_type_from_path(path) {
            Some(file_type @ FileType::Directory { .. }) => {
                let fd = unsafe { libc::dup(0) };
                self.fd_map.insert(fd, file_type);

                Some(FsDir { fd, offset: 0 })
            }
            _ => None,
        }
    }

    pub fn rewinddir(&mut self, dir: &mut FsDir) {
        dir.offset = 0;
    }
}

impl<'a> Drop for Fs<'a> {
    fn drop(&mut self) {
        for fd in self.fd_map.keys() {
            unsafe { libc::close(*fd) };
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn create_test_fs() -> Fs<'static> {
        let mut builder: TrieBuilder<&OsStr, &[u8]> = TrieBuilder::new();
        let ls = vec!["usr", "bin", "ls"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();
        let cat = vec!["usr", "bin", "cat"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();
        let hoge = vec!["usr", "bin", "hoge", "fuga"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();
        let fuga = vec!["usr", "bin", "fuga"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();
        let empty = vec!["usr", "empty"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();

        builder.push(&ls, b"ls_content");
        builder.push(&cat, b"cat_content_here");
        builder.push(&hoge, b"hoge_fuga_content");
        builder.push(&fuga, b"fuga_content");
        builder.push(&empty, b"");

        Fs::new(builder)
    }

    #[test]
    fn test_open_existing_file() {
        let mut fs = create_test_fs();
        let path = vec!["usr", "bin", "ls"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();

        let fd = fs.open(&path);
        assert!(fd.is_some());
        assert!(fd.unwrap() >= 0);
    }

    #[test]
    fn test_open_nonexistent_file() {
        let mut fs = create_test_fs();
        let path = vec!["usr", "bin", "nonexistent"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();

        let fd = fs.open(&path);
        assert!(fd.is_none());
    }

    #[test]
    fn test_read_file() {
        let mut fs = create_test_fs();
        let path = vec!["usr", "bin", "ls"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();

        let fd = fs.open(&path).unwrap();
        let mut buf = [0u8; 128];
        let read_size = fs.read(fd, &mut buf);

        assert!(read_size.is_some());
        assert_eq!(read_size.unwrap(), 10);
        assert_eq!(&buf[..10], b"ls_content");
    }

    #[test]
    fn test_read_partial() {
        let mut fs = create_test_fs();
        let path = vec!["usr", "bin", "cat"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();

        let fd = fs.open(&path).unwrap();

        // Read in small chunks
        let mut buf = [0u8; 4];
        let read_size = fs.read(fd, &mut buf).unwrap();
        assert_eq!(read_size, 4);
        assert_eq!(&buf, b"cat_");

        let read_size = fs.read(fd, &mut buf).unwrap();
        assert_eq!(read_size, 4);
        assert_eq!(&buf, b"cont");
    }

    #[test]
    fn test_read_empty_file() {
        let mut fs = create_test_fs();
        let path = vec!["usr", "empty"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();

        let fd = fs.open(&path).unwrap();
        let mut buf = [0u8; 128];
        let read_size = fs.read(fd, &mut buf);

        assert!(read_size.is_some());
        assert_eq!(read_size.unwrap(), 0);
    }

    #[test]
    fn test_read_eof() {
        let mut fs = create_test_fs();
        let path = vec!["usr", "bin", "ls"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();

        let fd = fs.open(&path).unwrap();
        let mut buf = [0u8; 128];

        // First read
        fs.read(fd, &mut buf);

        // Second read should return 0 (EOF)
        let read_size = fs.read(fd, &mut buf).unwrap();
        assert_eq!(read_size, 0);
    }

    #[test]
    fn test_read_invalid_fd() {
        let mut fs = create_test_fs();
        let mut buf = [0u8; 128];

        let result = fs.read(9999, &mut buf);
        assert!(result.is_none());
    }

    #[test]
    fn test_close() {
        let mut fs = create_test_fs();
        let path = vec!["usr", "bin", "ls"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();

        let fd = fs.open(&path).unwrap();
        assert!(fs.is_fd_exists(fd));

        let result = fs.close(fd);
        assert_eq!(result, 0);
        assert!(!fs.is_fd_exists(fd));
    }

    #[test]
    fn test_storage() {
        let mut builder: TrieBuilder<&OsStr, &[u8]> = TrieBuilder::new();
        let ls = vec!["usr", "bin", "ls"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();
        let cat = vec!["usr", "bin", "cat"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();
        let hoge = vec!["usr", "bin", "hoge", "fuga"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();
        let fuga = vec!["usr", "bin", "fuga"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();

        builder.push(&ls, &[1, 2, 3]);
        builder.push(&cat, &[4, 5, 6]);
        builder.push(&hoge, &[7, 8, 9]);
        builder.push(&fuga, &[10, 11, 12]);

        let fs = Fs::new(builder);

        let mut hasher = FxHasher::default();
        ls.hash(&mut hasher);

        assert_eq!(
            fs.get_file_type_from_path(&ls),
            Some(FileType::File {
                file: &[1, 2, 3],
                offset: 0,
                inode: hasher.finish()
            })
        );

        let mut hasher = FxHasher::default();
        let search_path = vec!["usr", "bin"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();

        search_path.clone().hash(&mut hasher);

        assert_eq!(
            fs.get_file_type_from_path(&search_path.clone()),
            Some(FileType::Directory {
                inode: hasher.finish(),
                entries: vec![
                    vec!["usr", "bin", "cat"]
                        .into_iter()
                        .map(OsString::from)
                        .collect(),
                    vec!["usr", "bin", "fuga"]
                        .into_iter()
                        .map(OsString::from)
                        .collect(),
                    vec!["usr", "bin", "hoge"]
                        .into_iter()
                        .map(OsString::from)
                        .collect(),
                    vec!["usr", "bin", "ls"]
                        .into_iter()
                        .map(OsString::from)
                        .collect(),
                ]
            })
        );

        let search_path = "usr/bin/cat"
            .split('/')
            .map(OsStr::new)
            .collect::<Vec<&OsStr>>();
        let mut hasher = FxHasher::default();
        vec!["usr", "bin", "cat"]
            .iter()
            .map(OsStr::new)
            .collect::<Vec<_>>()
            .hash(&mut hasher);

        assert_eq!(
            fs.get_file_type_from_path(&search_path),
            Some(FileType::File {
                file: &[4, 5, 6],
                offset: 0,
                inode: hasher.finish()
            })
        );
    }

    #[test]
    fn test_stat_file() {
        let fs = create_test_fs();
        let path = vec!["usr", "bin", "ls"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();

        let mut stat = unsafe { std::mem::zeroed::<libc::stat>() };
        let result = fs.stat(&path, &mut stat);

        assert_eq!(result, Some(0));
        assert_eq!(stat.st_size, 10); // "ls_content" = 10 bytes
        assert_eq!(stat.st_mode & libc::S_IFMT, libc::S_IFREG);
    }

    #[test]
    fn test_stat_directory() {
        let fs = create_test_fs();
        let path = vec!["usr", "bin"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();

        let mut stat = unsafe { std::mem::zeroed::<libc::stat>() };
        let result = fs.stat(&path, &mut stat);

        assert_eq!(result, Some(0));
        assert_eq!(stat.st_mode & libc::S_IFMT, libc::S_IFDIR);
    }

    #[test]
    fn test_stat_nonexistent() {
        let fs = create_test_fs();
        let path = vec!["nonexistent"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();

        let mut stat = unsafe { std::mem::zeroed::<libc::stat>() };
        let result = fs.stat(&path, &mut stat);

        assert!(result.is_none());
    }

    #[test]
    fn test_fstat() {
        let mut fs = create_test_fs();
        let path = vec!["usr", "bin", "cat"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();

        let fd = fs.open(&path).unwrap();
        let mut stat = unsafe { std::mem::zeroed::<libc::stat>() };
        let result = fs.fstat(fd, &mut stat);

        assert_eq!(result, Some(0));
        assert_eq!(stat.st_size, 16); // "cat_content_here" = 16 bytes
    }

    #[test]
    fn test_fstat_invalid_fd() {
        let fs = create_test_fs();
        let mut stat = unsafe { std::mem::zeroed::<libc::stat>() };
        let result = fs.fstat(9999, &mut stat);

        assert!(result.is_none());
    }

    #[test]
    fn test_lstat() {
        let fs = create_test_fs();
        let path = vec!["usr", "bin", "ls"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();

        let mut stat = unsafe { std::mem::zeroed::<libc::stat>() };
        let result = fs.lstat(&path, &mut stat);

        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_opendir() {
        let mut fs = create_test_fs();
        let path = vec!["usr", "bin"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();

        let dir = fs.opendir(&path);
        assert!(dir.is_some());

        let dir = dir.unwrap();
        assert!(dir.fd >= 0);
    }

    #[test]
    fn test_opendir_file_fails() {
        let mut fs = create_test_fs();
        let path = vec!["usr", "bin", "ls"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();

        let dir = fs.opendir(&path);
        assert!(dir.is_none());
    }

    #[test]
    fn test_opendir_nonexistent() {
        let mut fs = create_test_fs();
        let path = vec!["nonexistent"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();

        let dir = fs.opendir(&path);
        assert!(dir.is_none());
    }

    #[test]
    fn test_readdir() {
        let mut fs = create_test_fs();
        let path = vec!["usr", "bin"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();

        let mut dir = fs.opendir(&path).unwrap();

        // Read directory entries
        let mut entries = Vec::new();
        loop {
            let dirent = fs.readdir(&mut dir);
            assert!(dirent.is_some());

            let dirent_ptr = dirent.unwrap();
            if dirent_ptr.is_null() {
                break;
            }

            let dirent = unsafe { &*dirent_ptr };
            let name_bytes: Vec<u8> = dirent
                .d_name
                .iter()
                .take_while(|&&c| c != 0)
                .map(|&c| c as u8)
                .collect();
            let name = String::from_utf8_lossy(&name_bytes).to_string();
            entries.push(name);

            // Free the dirent
            unsafe { drop(Box::from_raw(dirent_ptr)) };
        }

        assert!(entries.contains(&"cat".to_string()));
        assert!(entries.contains(&"ls".to_string()));
        assert!(entries.contains(&"fuga".to_string()));
    }

    #[test]
    fn test_closedir() {
        let mut fs = create_test_fs();
        let path = vec!["usr", "bin"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();

        let dir = fs.opendir(&path).unwrap();
        let fd = dir.fd;

        assert!(fs.is_fd_exists(fd));

        let result = fs.closedir(&dir);
        assert_eq!(result, 0);
        assert!(!fs.is_fd_exists(fd));
    }

    #[test]
    fn test_rewinddir() {
        let mut fs = create_test_fs();
        let path = vec!["usr", "bin"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();

        let mut dir = fs.opendir(&path).unwrap();

        // Read first entry
        let first_entry = fs.readdir(&mut dir).unwrap();
        assert!(!first_entry.is_null());
        let first_name: Vec<u8> = unsafe { &*first_entry }
            .d_name
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u8)
            .collect();
        unsafe { drop(Box::from_raw(first_entry)) };

        // Read second entry
        let _ = fs.readdir(&mut dir);

        // Rewind
        fs.rewinddir(&mut dir);

        // Read first entry again
        let first_again = fs.readdir(&mut dir).unwrap();
        assert!(!first_again.is_null());
        let first_again_name: Vec<u8> = unsafe { &*first_again }
            .d_name
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u8)
            .collect();
        unsafe { drop(Box::from_raw(first_again)) };

        assert_eq!(first_name, first_again_name);
    }

    #[test]
    fn test_fdopendir() {
        let mut fs = create_test_fs();
        let path = vec!["usr", "bin"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();

        let fd = fs.open(&path).unwrap();
        let dir = fs.fdopendir(fd);

        assert!(dir.is_some());
        let dir = dir.unwrap();
        assert_eq!(dir.fd, fd);
    }

    #[test]
    fn test_fdopendir_file_fails() {
        let mut fs = create_test_fs();
        let path = vec!["usr", "bin", "ls"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();

        let fd = fs.open(&path).unwrap();
        let dir = fs.fdopendir(fd);

        assert!(dir.is_none());
    }

    #[test]
    fn test_is_fd_exists() {
        let mut fs = create_test_fs();
        let path = vec!["usr", "bin", "ls"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();

        assert!(!fs.is_fd_exists(9999));

        let fd = fs.open(&path).unwrap();
        assert!(fs.is_fd_exists(fd));

        fs.close(fd);
        assert!(!fs.is_fd_exists(fd));
    }

    #[test]
    fn test_is_dir_exists() {
        let mut fs = create_test_fs();
        let path = vec!["usr", "bin"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();

        let dir = fs.opendir(&path).unwrap();

        assert!(fs.is_dir_exists(&dir));
    }

    #[test]
    fn test_is_dir_exists_from_path() {
        let fs = create_test_fs();

        let dir_path = vec!["usr", "bin"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();
        assert!(fs.is_dir_exists_from_path(&dir_path));

        let file_path = vec!["usr", "bin", "ls"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();
        assert!(!fs.is_dir_exists_from_path(&file_path));

        let nonexistent = vec!["nonexistent"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();
        assert!(!fs.is_dir_exists_from_path(&nonexistent));
    }

    #[test]
    fn test_file_read() {
        let fs = create_test_fs();
        let path = vec!["usr", "bin", "ls"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();

        let ptr = fs.file_read(&path);
        assert!(ptr.is_some());

        let ptr = ptr.unwrap();
        let content = unsafe { std::slice::from_raw_parts(ptr, 10) };
        assert_eq!(content, b"ls_content");
    }

    #[test]
    fn test_open_at() {
        let mut fs = create_test_fs();
        let path = vec!["usr", "bin", "ls"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();

        let fd = fs.open_at(&path);
        assert!(fd.is_some());

        let fd = fd.unwrap();
        assert!(fs.is_fd_exists(fd));
    }

    #[test]
    fn test_multiple_opens() {
        let mut fs = create_test_fs();
        let path = vec!["usr", "bin", "ls"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();

        let fd1 = fs.open(&path).unwrap();
        let fd2 = fs.open(&path).unwrap();

        assert_ne!(fd1, fd2);
        assert!(fs.is_fd_exists(fd1));
        assert!(fs.is_fd_exists(fd2));

        fs.close(fd1);
        assert!(!fs.is_fd_exists(fd1));
        assert!(fs.is_fd_exists(fd2));
    }

    #[test]
    fn test_nested_directory() {
        let fs = create_test_fs();
        let path = vec!["usr", "bin", "hoge"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();

        assert!(fs.is_dir_exists_from_path(&path));
    }

    #[test]
    fn test_root_directory() {
        let fs = create_test_fs();
        let path = vec!["usr"].into_iter().map(OsStr::new).collect::<Vec<_>>();

        assert!(fs.is_dir_exists_from_path(&path));
    }
}
