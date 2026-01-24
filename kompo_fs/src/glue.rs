use std::{
    ffi::{CStr, CString},
    path::{Path, PathBuf},
    str::FromStr,
};

use crate::{FILE_TYPE_CACHE, TRIE, WORKING_DIR, initialize_trie, util};

#[unsafe(no_mangle)]
pub fn mmap_from_fs(
    addr: *mut libc::c_void,
    length: libc::size_t,
    prot: libc::c_int,
    flags: libc::c_int,
    fd: libc::c_int,
    offset: libc::off_t,
) -> *mut libc::c_void {
    if fd == -1 {
        return unsafe { kompo_wrap::MMAP_HANDLE(addr, length, prot, flags, fd, offset) };
    }

    if util::is_fd_exists_in_kompo(fd) {
        let mm = unsafe {
            kompo_wrap::MMAP_HANDLE(
                addr,
                length,
                libc::PROT_READ | libc::PROT_WRITE, // write by read_from_fs()
                libc::MAP_ANONYMOUS | libc::MAP_PRIVATE,
                -1,
                offset,
            )
        };

        if mm == libc::MAP_FAILED {
            return mm;
        }

        if read_from_fs(fd, mm, length) >= 0 {
            mm
        } else {
            errno::set_errno(errno::Errno(libc::EBADF));
            libc::MAP_FAILED
        }
    } else {
        unsafe { kompo_wrap::MMAP_HANDLE(addr, length, prot, flags, fd, offset) }
    }
}

#[unsafe(no_mangle)]
pub fn open_from_fs(path: *const libc::c_char, oflag: libc::c_int, mode: libc::mode_t) -> i32 {
    fn inner_open(path: *const libc::c_char, oflag: libc::c_int) -> libc::c_int {
        let path_cstr = unsafe { CStr::from_ptr(path) };
        let path_obj = Path::new(path_cstr.to_str().expect("invalid path"));
        let path_vec = path_obj.iter().collect::<Vec<_>>();

        let trie = std::sync::Arc::clone(TRIE.get_or_init(initialize_trie));

        #[cfg(target_os = "macos")]
        let o_directory = libc::O_DIRECTORY;
        #[cfg(target_os = "linux")]
        let o_directory = libc::O_DIRECTORY;

        if oflag & o_directory == o_directory {
            let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
            let trie_guard = trie.lock().unwrap();
            match trie_guard.stat(&path_vec, &mut stat_buf) {
                Some(_) => {
                    if stat_buf.st_mode & libc::S_IFMT == libc::S_IFDIR {
                        drop(trie_guard);
                        let mut trie = trie.lock().unwrap();
                        trie.open(&path_vec).unwrap_or_else(|| {
                            errno::set_errno(errno::Errno(libc::ENOENT));
                            -1
                        })
                    } else {
                        errno::set_errno(errno::Errno(libc::ENOTDIR));
                        -1
                    }
                }
                None => {
                    errno::set_errno(errno::Errno(libc::ENOENT));
                    -1
                }
            }
        } else {
            let mut trie = trie.lock().unwrap();
            trie.open(&path_vec).unwrap_or_else(|| {
                errno::set_errno(errno::Errno(libc::ENOENT));
                -1
            })
        }
    }

    if WORKING_DIR.read().unwrap().is_some() && unsafe { *path } != b'/'.try_into().unwrap() {
        let expand_path = unsafe { util::expand_kompo_path(path) };

        inner_open(expand_path, oflag)
    } else if unsafe { util::is_under_kompo_working_dir(path) } {
        inner_open(path, oflag)
    } else {
        unsafe { kompo_wrap::OPEN_HANDLE(path, oflag, mode) }
    }
}

#[unsafe(no_mangle)]
pub unsafe fn openat_from_fs(
    dirfd: libc::c_int,
    pathname: *const libc::c_char,
    flags: libc::c_int,
    mode: libc::mode_t,
) -> libc::c_int {
    fn inner_openat(
        _dirfd: libc::c_int,
        pathname: *const libc::c_char,
        _flags: libc::c_int,
        _mode: libc::mode_t,
    ) -> libc::c_int {
        let path = unsafe { CStr::from_ptr(pathname) };
        let path = PathBuf::from_str(path.to_str().expect("invalid path")).unwrap();

        let current_dir = WORKING_DIR.read().unwrap();
        let current_dir = current_dir.clone().expect("not found current dir");
        let mut current_dir = PathBuf::from(current_dir);

        util::canonicalize_path(&mut current_dir, &path);

        let path = current_dir.iter().collect::<Vec<_>>();

        let trie = std::sync::Arc::clone(TRIE.get_or_init(initialize_trie));
        let ret = {
            let mut trie = trie.lock().unwrap();

            trie.open(&path)
        };

        ret.unwrap_or_else(|| {
            errno::set_errno(errno::Errno(libc::ENOENT));
            -1
        })
    }

    #[cfg(target_os = "linux")]
    let is_create_flag =
        flags & libc::O_CREAT == libc::O_CREAT || flags & libc::O_TMPFILE == libc::O_TMPFILE;

    #[cfg(not(target_os = "linux"))]
    let is_create_flag = flags & libc::O_CREAT == libc::O_CREAT;

    if is_create_flag {
        return unsafe { kompo_wrap::OPENAT_HANDLE(dirfd, pathname, flags, mode) };
    }

    if unsafe { util::is_under_kompo_working_dir(pathname) } {
        return open_from_fs(pathname, flags, mode);
    }

    if dirfd == libc::AT_FDCWD
        && WORKING_DIR.read().unwrap().is_some()
        && unsafe { *pathname } != b'/'.try_into().unwrap()
    {
        return inner_openat(dirfd, pathname, flags, mode);
    }

    unsafe { kompo_wrap::OPENAT_HANDLE(dirfd, pathname, flags, mode) }
}

#[unsafe(no_mangle)]
pub fn close_from_fs(fd: i32) -> i32 {
    if util::is_fd_exists_in_kompo(fd) {
        std::sync::Arc::clone(TRIE.get_or_init(initialize_trie))
            .lock()
            .unwrap()
            .close(fd);
    };

    unsafe { kompo_wrap::CLOSE_HANDLE(fd) } // kompo_fs' inner fd made by dup(). so, close it.
}

#[unsafe(no_mangle)]
pub fn stat_from_fs(path: *const libc::c_char, stat: *mut libc::stat) -> i32 {
    fn inner_stat(path: *const libc::c_char, stat: *mut libc::stat) -> i32 {
        if stat.is_null() {
            errno::set_errno(errno::Errno(libc::EFAULT));
            return -1;
        }

        let path = unsafe { CStr::from_ptr(path) };
        let path = Path::new(path.to_str().expect("invalid path"));
        let path = path
            .iter()
            .map(|os_str| os_str.to_os_string())
            .collect::<Vec<_>>();

        // TODO: move to trie.stat()
        if let Some(cache) = FILE_TYPE_CACHE.read().unwrap().get(&path) {
            unsafe { *stat = *cache };
            return 0;
        }

        let sarch_path = path
            .iter()
            .map(|os_str| os_str.as_os_str())
            .collect::<Vec<_>>();

        let trie = std::sync::Arc::clone(TRIE.get_or_init(initialize_trie));
        {
            let trie = trie.lock().unwrap();
            let ret = trie.stat(&sarch_path, unsafe { &mut *stat });
            if ret.is_some() {
                unsafe { FILE_TYPE_CACHE.write().unwrap().insert(path, *stat) };
                0
            } else {
                errno::set_errno(errno::Errno(libc::ENOENT));
                -1
            }
        }
    }

    if WORKING_DIR.read().unwrap().is_some() && unsafe { *path } != b'/'.try_into().unwrap() {
        let expand_path = unsafe { util::expand_kompo_path(path) };

        inner_stat(expand_path, stat)
    } else if unsafe { util::is_under_kompo_working_dir(path) } {
        inner_stat(path, stat)
    } else {
        unsafe { kompo_wrap::STAT_HANDLE(path, stat) }
    }
}

#[unsafe(no_mangle)]
pub unsafe fn fstatat_from_fs(
    dirfd: libc::c_int,
    pathname: *const libc::c_char,
    buf: *mut libc::stat,
    flags: libc::c_int,
) -> i32 {
    fn inner_fstatat(
        _dirfd: libc::c_int,
        path: *const libc::c_char,
        stat: *mut libc::stat,
        _flags: libc::c_int,
    ) -> i32 {
        if stat.is_null() {
            errno::set_errno(errno::Errno(libc::EFAULT));
            return -1;
        }

        let path = unsafe { CStr::from_ptr(path) };
        let path = PathBuf::from_str(path.to_str().expect("invalid path")).expect("invalid path");

        let current_dir = WORKING_DIR.read().unwrap();
        let current_dir = current_dir.clone().expect("not found current dir");
        let mut current_dir = PathBuf::from(current_dir);

        util::canonicalize_path(&mut current_dir, &path);

        let sarch_path = current_dir.iter().collect::<Vec<_>>();

        let trie = std::sync::Arc::clone(TRIE.get_or_init(initialize_trie));
        {
            let trie = trie.lock().unwrap();
            let ret = trie.stat(&sarch_path, unsafe { &mut *stat });
            if ret.is_some() {
                0
            } else {
                errno::set_errno(errno::Errno(libc::ENOENT));
                -1
            }
        }
    }

    if unsafe { util::is_under_kompo_working_dir(pathname) } {
        return stat_from_fs(pathname, buf);
    }

    if dirfd == libc::AT_FDCWD
        && WORKING_DIR.read().unwrap().is_some()
        && unsafe { *pathname } != b'/'.try_into().unwrap()
    {
        return inner_fstatat(dirfd, pathname, buf, flags);
    }

    unsafe { kompo_wrap::FSTATAT_HANDLE(dirfd, pathname, buf, flags) }
}

#[unsafe(no_mangle)]
pub fn lstat_from_fs(path: *const libc::c_char, stat: *mut libc::stat) -> i32 {
    fn inner_lstat(path: *const libc::c_char, stat: *mut libc::stat) -> i32 {
        if stat.is_null() {
            errno::set_errno(errno::Errno(libc::EFAULT));
            return -1;
        }

        let path = unsafe { CStr::from_ptr(path) };
        let path = Path::new(path.to_str().expect("invalid path"));
        let path = path
            .iter()
            .map(|os_str| os_str.to_os_string())
            .collect::<Vec<_>>();

        // TODO: move to trie.stat()
        if let Some(cache) = FILE_TYPE_CACHE.read().unwrap().get(&path) {
            unsafe { *stat = *cache };
            return 0;
        }

        let sarch_path = path
            .iter()
            .map(|os_str| os_str.as_os_str())
            .collect::<Vec<_>>();

        let trie = std::sync::Arc::clone(TRIE.get_or_init(initialize_trie));
        {
            let trie = trie.lock().unwrap();
            let ret = trie.lstat(&sarch_path, unsafe { &mut *stat });
            if ret.is_some() {
                unsafe { FILE_TYPE_CACHE.write().unwrap().insert(path, *stat) };
                0
            } else {
                errno::set_errno(errno::Errno(libc::ENOENT));
                -1
            }
        }
    }

    if WORKING_DIR.read().unwrap().is_some() && unsafe { *path } != b'/'.try_into().unwrap() {
        let expand_path = unsafe { util::expand_kompo_path(path) };

        inner_lstat(expand_path, stat)
    } else if unsafe { util::is_under_kompo_working_dir(path) } {
        inner_lstat(path, stat)
    } else {
        unsafe { kompo_wrap::LSTAT_HANDLE(path, stat) }
    }
}

#[unsafe(no_mangle)]
pub fn fstat_from_fs(fd: i32, stat: *mut libc::stat) -> i32 {
    fn inner_fstat(fd: i32, stat: *mut libc::stat) -> i32 {
        if stat.is_null() {
            errno::set_errno(errno::Errno(libc::EFAULT));
            return -1;
        }

        let trie = std::sync::Arc::clone(TRIE.get_or_init(initialize_trie));
        let ret = trie.lock().unwrap().fstat(fd, unsafe { &mut *stat });

        if ret.is_some() {
            0
        } else {
            errno::set_errno(errno::Errno(libc::ENOENT));
            -1
        }
    }

    if util::is_fd_exists_in_kompo(fd) {
        inner_fstat(fd, stat)
    } else {
        unsafe { kompo_wrap::FSTAT_HANDLE(fd, stat) }
    }
}

#[unsafe(no_mangle)]
pub fn read_from_fs(fd: i32, buf: *mut libc::c_void, count: libc::size_t) -> isize {
    fn inner_read(fd: i32, buf: *mut libc::c_void, count: libc::size_t) -> isize {
        let buf = unsafe { std::slice::from_raw_parts_mut(buf as *mut u8, count) };

        let trie = std::sync::Arc::clone(TRIE.get_or_init(initialize_trie));
        let ret = trie.lock().expect("trie is poisoned").read(fd, buf);

        if let Some(read_bytes) = ret {
            read_bytes
        } else {
            errno::set_errno(errno::Errno(libc::ENOENT));
            -1
        }
    }

    if util::is_fd_exists_in_kompo(fd) {
        inner_read(fd, buf, count)
    } else {
        unsafe { kompo_wrap::READ_HANDLE(fd, buf, count) }
    }
}

#[unsafe(no_mangle)]
pub fn getcwd_from_fs(buf: *mut libc::c_char, count: libc::size_t) -> *const libc::c_char {
    fn inner_getcwd(buf: *mut libc::c_char, count: libc::size_t) -> *const libc::c_char {
        let working_dir = WORKING_DIR.read().unwrap();

        if working_dir.is_none() {
            return std::ptr::null();
        }

        let working_dir = working_dir.clone().unwrap();

        if buf.is_null() {
            if count == 0 {
                let working_directory_path =
                    CString::new(working_dir.to_str().expect("invalid path"))
                        .expect("invalid path")
                        .into_boxed_c_str();
                let ptr = Box::into_raw(working_directory_path);

                ptr as *const libc::c_char
            } else {
                todo!()
            }
        } else {
            todo!()
        }
    }

    if WORKING_DIR.read().unwrap().is_some() {
        inner_getcwd(buf, count)
    } else {
        unsafe { kompo_wrap::GETCWD_HANDLE(buf, count) }
    }
}

#[unsafe(no_mangle)]
pub fn chdir_from_fs(path: *const libc::c_char) -> libc::c_int {
    fn inner_chdir(path: *const libc::c_char) -> libc::c_int {
        let path = unsafe { CStr::from_ptr(path) };
        let path = Path::new(path.to_str().expect("invalid path"));

        let search_path = path.iter().collect::<Vec<_>>();
        let trie = std::sync::Arc::clone(TRIE.get_or_init(initialize_trie));
        let bool = trie
            .lock()
            .expect("trie is poisoned")
            .is_dir_exists_from_path(&search_path);

        if bool {
            let changed_path = path.as_os_str().to_os_string();
            *WORKING_DIR.write().unwrap() = Some(changed_path);

            1
        } else {
            -1
        }
    }

    let change_dir = unsafe { util::expand_kompo_path(path) };

    if unsafe { util::is_under_kompo_working_dir(change_dir) } {
        inner_chdir(change_dir)
    } else {
        let ret = unsafe { kompo_wrap::CHDIR_HANDLE(path) };
        if ret == 0 {
            *WORKING_DIR.write().unwrap() = None;
        }

        ret
    }
}

#[unsafe(no_mangle)]
pub fn fdopendir_from_fs(fd: i32) -> *mut libc::DIR {
    fn inner_fdopendir(fd: i32) -> *mut libc::DIR {
        let trie = std::sync::Arc::clone(TRIE.get_or_init(initialize_trie));
        {
            let trie = trie.lock().unwrap();

            match trie.fdopendir(fd) {
                Some(dir) => {
                    let dir = Box::new(dir);
                    Box::into_raw(dir) as *mut libc::DIR
                }
                None => std::ptr::null_mut(),
            }
        }
    }

    if util::is_fd_exists_in_kompo(fd) {
        inner_fdopendir(fd)
    } else {
        unsafe { kompo_wrap::FDOPENDIR_HANDLE(fd) }
    }
}

#[unsafe(no_mangle)]
pub fn readdir_from_fs(dir: *mut libc::DIR) -> *mut libc::dirent {
    fn inner_readdir(dir: *mut libc::DIR) -> *mut libc::dirent {
        let mut dir = unsafe { Box::from_raw(dir as *mut kompo_storage::FsDir) };

        let trie = std::sync::Arc::clone(TRIE.get_or_init(initialize_trie));
        {
            let trie = trie.lock().unwrap();

            match trie.readdir(&mut dir) {
                Some(dirent) => {
                    let _ = Box::into_raw(dir);
                    dirent
                }
                None => {
                    let _ = Box::into_raw(dir);
                    std::ptr::null_mut()
                }
            }
        }
    }

    if unsafe { util::is_dir_exists_in_kompo(dir) } {
        inner_readdir(dir)
    } else {
        unsafe { kompo_wrap::READDIR_HANDLE(dir) }
    }
}

#[unsafe(no_mangle)]
pub fn closedir_from_fs(dir: *mut libc::DIR) -> i32 {
    if unsafe { util::is_dir_exists_in_kompo(dir) } {
        let dir = unsafe { Box::from_raw(dir as *mut kompo_storage::FsDir) };
        std::sync::Arc::clone(TRIE.get_or_init(initialize_trie))
            .lock()
            .unwrap()
            .closedir(&dir);

        unsafe { kompo_wrap::CLOSE_HANDLE(dir.fd) }
    } else {
        unsafe { kompo_wrap::CLOSEDIR_HANDLE(dir) }
    }
}

#[unsafe(no_mangle)]
pub fn opendir_from_fs(path: *const libc::c_char) -> *mut libc::DIR {
    fn inner_opendir(path: *const libc::c_char) -> *mut libc::DIR {
        let path_cstr = unsafe { CStr::from_ptr(path) };
        let path_str = path_cstr.to_str().expect("invalid path");
        let path = Path::new(path_str);
        let path = path.iter().collect::<Vec<_>>();

        let trie = std::sync::Arc::clone(TRIE.get_or_init(initialize_trie));
        {
            let mut trie = trie.lock().unwrap();

            match trie.opendir(&path) {
                Some(dir) => {
                    let dir = Box::new(dir);
                    Box::into_raw(dir) as *mut libc::DIR
                }
                None => std::ptr::null_mut(),
            }
        }
    }

    if WORKING_DIR.read().unwrap().is_some() && unsafe { *path } != b'/'.try_into().unwrap() {
        let expand_path = unsafe { util::expand_kompo_path(path) };
        inner_opendir(expand_path)
    } else if unsafe { util::is_under_kompo_working_dir(path) } {
        inner_opendir(path)
    } else {
        unsafe { kompo_wrap::OPENDIR_HANDLE(path) }
    }
}

#[unsafe(no_mangle)]
pub fn rewinddir_from_fs(dir: *mut libc::DIR) {
    fn inner_rewinddir(dir: *mut libc::DIR) {
        let mut dir = unsafe { Box::from_raw(dir as *mut kompo_storage::FsDir) };

        let trie = std::sync::Arc::clone(TRIE.get_or_init(initialize_trie));
        {
            let mut trie = trie.lock().unwrap();

            trie.rewinddir(&mut dir);
            let _ = Box::into_raw(dir);
        }
    }

    if unsafe { util::is_dir_exists_in_kompo(dir) } {
        inner_rewinddir(dir)
    } else {
        unsafe { kompo_wrap::REWINDDIR_HANDLE(dir) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn realpath_from_fs(
    path: *const libc::c_char,
    resolved_path: *mut libc::c_char,
) -> *const libc::c_char {
    unsafe fn inner_realpath(
        path: *const libc::c_char,
        resolved_path: *mut libc::c_char,
    ) -> *const libc::c_char {
        if resolved_path.is_null() {
            unsafe { util::expand_kompo_path(path) }
        } else {
            let expand_path = unsafe { CStr::from_ptr(util::expand_kompo_path(path)) };
            let bytes = expand_path.to_bytes_with_nul();
            unsafe {
                std::ptr::copy_nonoverlapping(
                    bytes.as_ptr() as *const libc::c_char,
                    resolved_path,
                    bytes.len(),
                );
            }

            resolved_path
        }
    }

    if (WORKING_DIR.read().unwrap().is_some() && unsafe { *path } != b'/'.try_into().unwrap())
        || unsafe { util::is_under_kompo_working_dir(path) }
    {
        unsafe { inner_realpath(path, resolved_path) }
    } else {
        unsafe { kompo_wrap::REALPATH_HANDLE(path, resolved_path) }
    }
}

#[unsafe(no_mangle)]
pub fn mkdir_from_fs(path: *const libc::c_char, mode: libc::mode_t) -> libc::c_int {
    fn inner_mkdir(path: *const libc::c_char) -> libc::c_int {
        let layout = std::alloc::Layout::new::<libc::stat>();
        let stat_buf = unsafe { std::alloc::alloc(layout) as *mut libc::stat };

        let ret = stat_from_fs(path, stat_buf);

        unsafe { std::alloc::dealloc(stat_buf as *mut u8, layout) };

        if ret == 0 {
            return 0;
        }

        errno::set_errno(errno::Errno(libc::ENOENT));
        -1
    }

    if WORKING_DIR.read().unwrap().is_some() && unsafe { *path } != b'/'.try_into().unwrap() {
        let expand_path = unsafe { util::expand_kompo_path(path) };
        inner_mkdir(expand_path)
    } else if unsafe { util::is_under_kompo_working_dir(path) } {
        inner_mkdir(path)
    } else {
        unsafe { kompo_wrap::MKDIR_HANDLE(path, mode) }
    }
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub fn getattrlist_from_fs(
    path: *const libc::c_char,
    attr_list: *mut libc::c_void,
    attr_buf: *mut libc::c_void,
    attr_buf_size: libc::size_t,
    options: libc::c_ulong,
) -> libc::c_int {
    fn inner_getattrlist(
        path: *const libc::c_char,
        attr_list: *mut libc::c_void,
        attr_buf: *mut libc::c_void,
        attr_buf_size: libc::size_t,
    ) -> libc::c_int {
        let path_cstr = unsafe { CStr::from_ptr(path) };
        let path_path = Path::new(path_cstr.to_str().expect("invalid path"));
        let search_path = path_path.iter().collect::<Vec<_>>();

        let trie = std::sync::Arc::clone(TRIE.get_or_init(initialize_trie));
        let trie_guard = trie.lock().unwrap();

        let ret = trie_guard.getattrlist(
            &search_path,
            unsafe { &*(attr_list as *const libc::attrlist) },
            attr_buf,
            attr_buf_size,
        );

        match ret {
            Some(r) => r,
            None => {
                errno::set_errno(errno::Errno(libc::ENOENT));
                -1
            }
        }
    }

    if WORKING_DIR.read().unwrap().is_some() && unsafe { *path } != b'/'.try_into().unwrap() {
        let expand_path = unsafe { util::expand_kompo_path(path) };
        inner_getattrlist(expand_path, attr_list, attr_buf, attr_buf_size)
    } else if unsafe { util::is_under_kompo_working_dir(path) } {
        inner_getattrlist(path, attr_list, attr_buf, attr_buf_size)
    } else {
        unsafe { kompo_wrap::GETATTRLIST_HANDLE(path, attr_list, attr_buf, attr_buf_size, options) }
    }
}
