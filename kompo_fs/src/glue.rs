use std::{
    borrow::Cow,
    ffi::{CStr, CString},
    path::{Path, PathBuf},
    str::FromStr,
};

use crate::{initialize_trie, util, FILE_TYPE_CACHE, TRIE, WORKING_DIR};

#[no_mangle]
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

#[no_mangle]
pub fn open_from_fs(path: *const libc::c_char, oflag: libc::c_int, mode: libc::mode_t) -> i32 {
    fn inner_open(path: *const libc::c_char) -> libc::c_int {
        let path = unsafe { CStr::from_ptr(path) };
        let path = Path::new(path.to_str().expect("invalid path"));
        let path = path.iter().collect::<Vec<_>>();

        let trie = std::sync::Arc::clone(&TRIE.get_or_init(initialize_trie));
        let ret = {
            let mut trie = trie.lock().unwrap();

            trie.open(&path)
        };

        ret.unwrap_or_else(|| {
            errno::set_errno(errno::Errno(libc::ENOENT));
            -1
        })
    }

    if unsafe { WORKING_DIR.borrow().is_some() } && unsafe { *path } != b'/' {
        let expand_path = util::expand_kompo_path(path);

        inner_open(expand_path)
    } else if util::is_under_kompo_working_dir(path) {
        inner_open(path)
    } else {
        unsafe { kompo_wrap::OPEN_HANDLE(path, oflag, mode) }
    }
}

#[no_mangle]
pub unsafe fn openat_from_fs(
    dirfd: libc::c_int,
    pathname: *const libc::c_char,
    flags: libc::c_int,
    mode: libc::mode_t,
) -> libc::c_int {
    fn inner_openat(
        dirfd: libc::c_int,
        pathname: *const libc::c_char,
        flags: libc::c_int,
        mode: libc::mode_t,
    ) -> libc::c_int {
        let path = unsafe { CStr::from_ptr(pathname) };
        let path = PathBuf::from_str(path.to_str().expect("invalid path")).unwrap();

        let current_dir = unsafe { WORKING_DIR.borrow() };
        let current_dir = current_dir.clone().expect("not found current dir");
        let mut current_dir = PathBuf::from(current_dir.into_owned());

        util::canonicalize_path(&mut current_dir, &path);

        let path = current_dir.iter().collect::<Vec<_>>();

        let trie = std::sync::Arc::clone(&TRIE.get_or_init(initialize_trie));
        let ret = {
            let mut trie = trie.lock().unwrap();

            trie.open(&path)
        };

        ret.unwrap_or_else(|| {
            errno::set_errno(errno::Errno(libc::ENOENT));
            -1
        })
    }

    if flags & libc::O_CREAT == libc::O_CREAT || flags & libc::O_TMPFILE == libc::O_TMPFILE {
        return kompo_wrap::OPENAT_HANDLE(dirfd, pathname, flags, mode);
    }

    if util::is_under_kompo_working_dir(pathname) {
        return open_from_fs(pathname, flags, mode);
    }

    if dirfd == libc::AT_FDCWD && WORKING_DIR.borrow().is_some() && *pathname != b'/' {
        return inner_openat(dirfd, pathname, flags, mode);
    }

    kompo_wrap::OPENAT_HANDLE(dirfd, pathname, flags, mode)
}

#[no_mangle]
pub fn close_from_fs(fd: i32) -> i32 {
    if util::is_fd_exists_in_kompo(fd) {
        std::sync::Arc::clone(&TRIE.get_or_init(initialize_trie))
            .lock()
            .unwrap()
            .close(fd);
    };

    unsafe { kompo_wrap::CLOSE_HANDLE(fd) } // kompo_fs' inner fd made by dup(). so, close it.
}

#[no_mangle]
pub fn stat_from_fs(path: *const libc::c_char, stat: *mut libc::stat) -> i32 {
    fn inner_stat(path: *const libc::c_char, stat: *mut libc::stat) -> i32 {
        let path = unsafe { CStr::from_ptr(path) };
        let path = Path::new(path.to_str().expect("invalid path"));
        let path = path
            .iter()
            .map(|os_str| os_str.to_os_string())
            .collect::<Vec<_>>();

        // TODO: move to trie.stat()
        if let Some(cache) = unsafe { FILE_TYPE_CACHE.read().unwrap().get(&path) } {
            unsafe { *stat = cache.clone() };
            return 0;
        }

        let sarch_path = path
            .iter()
            .map(|os_str| os_str.as_os_str())
            .collect::<Vec<_>>();

        let trie = std::sync::Arc::clone(&TRIE.get_or_init(initialize_trie));
        {
            let trie = trie.lock().unwrap();
            let ret = trie.stat(&sarch_path, stat);
            if ret.is_some() {
                unsafe {
                    FILE_TYPE_CACHE
                        .write()
                        .unwrap()
                        .insert(path, (*stat).clone())
                };
                0
            } else {
                errno::set_errno(errno::Errno(libc::ENOENT));
                -1
            }
        }
    }

    if unsafe { WORKING_DIR.borrow().is_some() } && unsafe { *path } != b'/' {
        let expand_path = util::expand_kompo_path(path);

        inner_stat(expand_path, stat)
    } else if util::is_under_kompo_working_dir(path) {
        inner_stat(path, stat)
    } else {
        unsafe { kompo_wrap::STAT_HANDLE(path, stat) }
    }
}

#[no_mangle]
pub unsafe fn fstatat_from_fs(
    dirfd: libc::c_int,
    pathname: *const libc::c_char,
    buf: *mut libc::stat,
    flags: libc::c_int,
) -> i32 {
    fn inner_fstatat(
        dirfd: libc::c_int,
        path: *const libc::c_char,
        stat: *mut libc::stat,
        flags: libc::c_int,
    ) -> i32 {
        let path = unsafe { CStr::from_ptr(path) };
        let path = PathBuf::from_str(path.to_str().expect("invalid path")).expect("invalid path");

        let current_dir = unsafe { WORKING_DIR.borrow() };
        let current_dir = current_dir.clone().expect("not found current dir");
        let mut current_dir = PathBuf::from(current_dir.into_owned());

        util::canonicalize_path(&mut current_dir, &path);

        let sarch_path = current_dir.iter().collect::<Vec<_>>();

        let trie = std::sync::Arc::clone(&TRIE.get_or_init(initialize_trie));
        {
            let trie = trie.lock().unwrap();
            let ret = trie.stat(&sarch_path, stat);
            if ret.is_some() {
                0
            } else {
                errno::set_errno(errno::Errno(libc::ENOENT));
                -1
            }
        }
    }

    if util::is_under_kompo_working_dir(pathname) {
        return stat_from_fs(pathname, buf);
    }

    if dirfd == libc::AT_FDCWD && WORKING_DIR.borrow().is_some() && *pathname != b'/' {
        return inner_fstatat(dirfd, pathname, buf, flags);
    }

    kompo_wrap::FSTATAT_HANDLE(dirfd, pathname, buf, flags)
}

#[no_mangle]
pub fn lstat_from_fs(path: *const libc::c_char, stat: *mut libc::stat) -> i32 {
    fn inner_lstat(path: *const libc::c_char, stat: *mut libc::stat) -> i32 {
        let path = unsafe { CStr::from_ptr(path) };
        let path = Path::new(path.to_str().expect("invalid path"));
        let path = path
            .iter()
            .map(|os_str| os_str.to_os_string())
            .collect::<Vec<_>>();

        // TODO: move to trie.stat()
        if let Some(cache) = unsafe { FILE_TYPE_CACHE.read().unwrap().get(&path) } {
            unsafe { *stat = cache.clone() };
            return 0;
        }

        let sarch_path = path
            .iter()
            .map(|os_str| os_str.as_os_str())
            .collect::<Vec<_>>();

        let trie = std::sync::Arc::clone(&TRIE.get_or_init(initialize_trie));
        {
            let trie = trie.lock().unwrap();
            let ret = trie.lstat(&sarch_path, stat);
            if ret.is_some() {
                unsafe {
                    FILE_TYPE_CACHE
                        .write()
                        .unwrap()
                        .insert(path, (*stat).clone())
                };
                0
            } else {
                errno::set_errno(errno::Errno(libc::ENOENT));
                -1
            }
        }
    }

    if unsafe { WORKING_DIR.borrow().is_some() } && unsafe { *path } != b'/' {
        let expand_path = util::expand_kompo_path(path);

        inner_lstat(expand_path, stat)
    } else if util::is_under_kompo_working_dir(path) {
        inner_lstat(path, stat)
    } else {
        unsafe { kompo_wrap::LSTAT_HANDLE(path, stat) }
    }
}

#[no_mangle]
pub fn fstat_from_fs(fd: i32, stat: *mut libc::stat) -> i32 {
    fn inner_fstat(fd: i32, stat: *mut libc::stat) -> i32 {
        let trie = std::sync::Arc::clone(&TRIE.get_or_init(initialize_trie));
        let ret = trie.lock().unwrap().fstat(fd, stat);

        if ret.is_some() {
            ret.unwrap()
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

#[no_mangle]
pub fn read_from_fs(fd: i32, buf: *mut libc::c_void, count: libc::size_t) -> isize {
    fn inner_read(fd: i32, buf: *mut libc::c_void, count: libc::size_t) -> isize {
        let mut buf = unsafe { std::slice::from_raw_parts_mut(buf as *mut u8, count as usize) };

        let trie = std::sync::Arc::clone(&TRIE.get_or_init(initialize_trie));
        let ret = trie.lock().expect("trie is poisoned").read(fd, &mut buf);

        if ret.is_some() {
            ret.unwrap() as isize
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

#[no_mangle]
pub fn getcwd_from_fs(buf: *mut libc::c_char, count: libc::size_t) -> *const libc::c_char {
    fn inner_getcwd(buf: *mut libc::c_char, count: libc::size_t) -> *const libc::c_char {
        let working_dir = unsafe { WORKING_DIR.borrow() };

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

    if unsafe { WORKING_DIR.borrow().is_some() } {
        inner_getcwd(buf, count)
    } else {
        unsafe { kompo_wrap::GETCWD_HANDLE(buf, count) }
    }
}

#[no_mangle]
pub fn chdir_from_fs(path: *const libc::c_char) -> libc::c_int {
    fn inner_chdir(path: *const libc::c_char) -> libc::c_int {
        let path = unsafe { CStr::from_ptr(path) };
        let path = Path::new(path.to_str().expect("invalid path"));

        let search_path = path.iter().collect::<Vec<_>>();
        let trie = std::sync::Arc::clone(&TRIE.get_or_init(initialize_trie));
        let bool = trie
            .lock()
            .expect("trie is poisoned")
            .is_dir_exists_from_path(&search_path);

        if bool {
            unsafe {
                let changed_path = path.as_os_str().to_os_string();
                let changed_path = Cow::Owned(changed_path);
                WORKING_DIR.replace(Some(changed_path));
            }

            1
        } else {
            -1
        }
    }

    let change_dir = util::expand_kompo_path(path);

    if util::is_under_kompo_working_dir(change_dir) {
        inner_chdir(change_dir)
    } else {
        let ret = unsafe { kompo_wrap::CHDIR_HANDLE(path) };
        if ret == 0 {
            unsafe { WORKING_DIR.replace(None) };
        }

        ret
    }
}

#[no_mangle]
pub fn fdopendir_from_fs(fd: i32) -> *mut libc::DIR {
    fn inner_fdopendir(fd: i32) -> *mut libc::DIR {
        let trie = std::sync::Arc::clone(&TRIE.get_or_init(initialize_trie));
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

#[no_mangle]
pub fn readdir_from_fs(dir: *mut libc::DIR) -> *mut libc::dirent {
    fn inner_readdir(dir: *mut libc::DIR) -> *mut libc::dirent {
        let mut dir = unsafe { Box::from_raw(dir as *mut kompo_storage::FsDir) };

        let trie = std::sync::Arc::clone(&TRIE.get_or_init(initialize_trie));
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

    if util::is_dir_exists_in_kompo(dir) {
        inner_readdir(dir)
    } else {
        unsafe { kompo_wrap::READDIR_HANDLE(dir) }
    }
}

#[no_mangle]
pub fn closedir_from_fs(dir: *mut libc::DIR) -> i32 {
    if util::is_dir_exists_in_kompo(dir) {
        let dir = unsafe { Box::from_raw(dir as *mut kompo_storage::FsDir) };
        std::sync::Arc::clone(&TRIE.get_or_init(initialize_trie))
            .lock()
            .unwrap()
            .closedir(&dir);

        unsafe { kompo_wrap::CLOSE_HANDLE(dir.fd) }
    } else {
        unsafe { kompo_wrap::CLOSEDIR_HANDLE(dir) }
    }
}

#[no_mangle]
pub fn opendir_from_fs(path: *const libc::c_char) -> *mut libc::DIR {
    fn inner_opendir(path: *const libc::c_char) -> *mut libc::DIR {
        let path = unsafe { CStr::from_ptr(path) };
        let path = Path::new(path.to_str().expect("invalid path"));
        let path = path.iter().collect::<Vec<_>>();

        let trie = std::sync::Arc::clone(&TRIE.get_or_init(initialize_trie));
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

    if unsafe { WORKING_DIR.borrow().is_some() } && unsafe { *path } != b'/' {
        let expand_path = util::expand_kompo_path(path);

        inner_opendir(expand_path)
    } else if util::is_under_kompo_working_dir(path) {
        inner_opendir(path)
    } else {
        unsafe { kompo_wrap::OPENDIR_HANDLE(path) }
    }
}

#[no_mangle]
pub fn rewinddir_from_fs(dir: *mut libc::DIR) {
    fn inner_rewinddir(dir: *mut libc::DIR) {
        let mut dir = unsafe { Box::from_raw(dir as *mut kompo_storage::FsDir) };

        let trie = std::sync::Arc::clone(&TRIE.get_or_init(initialize_trie));
        {
            let mut trie = trie.lock().unwrap();

            trie.rewinddir(&mut dir);
            let _ = Box::into_raw(dir);
        }
    }

    if util::is_dir_exists_in_kompo(dir) {
        inner_rewinddir(dir)
    } else {
        unsafe { kompo_wrap::REWINDDIR_HANDLE(dir) }
    }
}

#[no_mangle]
pub unsafe extern "C-unwind" fn realpath_from_fs(
    path: *const libc::c_char,
    resolved_path: *mut libc::c_char,
) -> *const libc::c_char {
    fn inner_realpath(
        path: *const libc::c_char,
        resolved_path: *mut libc::c_char,
    ) -> *const libc::c_char {
        if resolved_path.is_null() {
            let expand_path = util::expand_kompo_path(path);

            expand_path
        } else {
            let expand_path = unsafe { CStr::from_ptr(util::expand_kompo_path(path)) };
            let buf =
                unsafe { std::slice::from_raw_parts_mut(resolved_path, libc::PATH_MAX as usize) };
            buf.copy_from_slice(expand_path.to_bytes_with_nul());

            buf.as_ptr()
        }
    }

    if WORKING_DIR.borrow().is_some() && *path != b'/' {
        inner_realpath(path, resolved_path)
    } else if util::is_under_kompo_working_dir(path) {
        inner_realpath(path, resolved_path)
    } else {
        unsafe { kompo_wrap::REALPATH_HANDLE(path, resolved_path) }
    }
}

#[no_mangle]
pub fn mkdir_from_fs(path: *const libc::c_char, mode: libc::mode_t) -> libc::c_int {
    let layout = std::alloc::Layout::new::<libc::stat>();
    let stat = unsafe { std::alloc::alloc(layout) as *mut libc::stat };

    let ret = stat_from_fs(path, stat);

    ret
}
