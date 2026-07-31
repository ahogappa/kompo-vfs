//! Interposed libc entry points.
//!
//! Each one decides whether the path belongs to the packed image and either
//! answers it from [`crate::fs`] or hands the original arguments to the real
//! libc function.
//!
//! Paths are handled as bytes throughout. The previous version went through
//! `CStr::to_str()`, which validates UTF-8 on every intercepted call and panics
//! on a filename that is not valid UTF-8 -- a Latin-1 name inside a gem is
//! enough, since nothing validates encoding when the image is built.

use std::borrow::Cow;
use std::ffi::CString;

use crate::{WORKING_DIR, fs, util};

fn enoent() -> i32 {
    errno::set_errno(errno::Errno(libc::ENOENT));
    -1
}

#[unsafe(no_mangle)]
pub fn mmap_from_fs(
    addr: *mut libc::c_void,
    length: libc::size_t,
    prot: libc::c_int,
    flags: libc::c_int,
    fd: libc::c_int,
    offset: libc::off_t,
) -> *mut libc::c_void {
    if fd == -1 || !util::is_fd_exists_in_kompo(fd) {
        return unsafe { kompo_wrap::MMAP_HANDLE(addr, length, prot, flags, fd, offset) };
    }

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
}

fn open_resolved(path: &[u8], oflag: libc::c_int) -> i32 {
    let fs = fs();
    let Some(node) = fs.lookup(path) else {
        return enoent();
    };

    if oflag & libc::O_DIRECTORY == libc::O_DIRECTORY && !fs.is_dir(node) {
        errno::set_errno(errno::Errno(libc::ENOTDIR));
        return -1;
    }

    fs.open_node(node).unwrap_or_else(enoent)
}

#[unsafe(no_mangle)]
pub fn open_from_fs(path: *const libc::c_char, oflag: libc::c_int, mode: libc::mode_t) -> i32 {
    let raw = unsafe { util::path_bytes(path) };

    match util::kompo_path(raw) {
        Some(resolved) => open_resolved(&resolved, oflag),
        None => unsafe { kompo_wrap::OPEN_HANDLE(path, oflag, mode) },
    }
}

/// # Safety
/// `pathname` must be a valid pointer to a null-terminated C string.
#[unsafe(no_mangle)]
pub unsafe fn openat_from_fs(
    dirfd: libc::c_int,
    pathname: *const libc::c_char,
    flags: libc::c_int,
    mode: libc::mode_t,
) -> libc::c_int {
    #[cfg(target_os = "linux")]
    let is_create_flag =
        flags & libc::O_CREAT == libc::O_CREAT || flags & libc::O_TMPFILE == libc::O_TMPFILE;
    #[cfg(not(target_os = "linux"))]
    let is_create_flag = flags & libc::O_CREAT == libc::O_CREAT;

    if is_create_flag {
        return unsafe { kompo_wrap::OPENAT_HANDLE(dirfd, pathname, flags, mode) };
    }

    let raw = unsafe { util::path_bytes(pathname) };

    match util::kompo_path_at(dirfd, raw) {
        Some(resolved) => open_resolved(&resolved, flags),
        None => unsafe { kompo_wrap::OPENAT_HANDLE(dirfd, pathname, flags, mode) },
    }
}

#[unsafe(no_mangle)]
pub fn close_from_fs(fd: i32) -> i32 {
    if util::is_fd_exists_in_kompo(fd) {
        fs().close(fd);
    }

    unsafe { kompo_wrap::CLOSE_HANDLE(fd) } // the inner fd came from dup(), so close it
}

fn stat_resolved(path: &[u8], stat: *mut libc::stat) -> i32 {
    if stat.is_null() {
        errno::set_errno(errno::Errno(libc::EFAULT));
        return -1;
    }

    match fs().stat(path, unsafe { &mut *stat }) {
        Some(_) => 0,
        None => enoent(),
    }
}

#[unsafe(no_mangle)]
pub fn stat_from_fs(path: *const libc::c_char, stat: *mut libc::stat) -> i32 {
    let raw = unsafe { util::path_bytes(path) };

    match util::kompo_path(raw) {
        Some(resolved) => stat_resolved(&resolved, stat),
        None => unsafe { kompo_wrap::STAT_HANDLE(path, stat) },
    }
}

/// The image holds no symlinks, so this is `stat`.
#[unsafe(no_mangle)]
pub fn lstat_from_fs(path: *const libc::c_char, stat: *mut libc::stat) -> i32 {
    let raw = unsafe { util::path_bytes(path) };

    match util::kompo_path(raw) {
        Some(resolved) => stat_resolved(&resolved, stat),
        None => unsafe { kompo_wrap::LSTAT_HANDLE(path, stat) },
    }
}

/// # Safety
/// `pathname` must be a valid pointer to a null-terminated C string.
#[unsafe(no_mangle)]
pub unsafe fn fstatat_from_fs(
    dirfd: libc::c_int,
    pathname: *const libc::c_char,
    buf: *mut libc::stat,
    flags: libc::c_int,
) -> i32 {
    let raw = unsafe { util::path_bytes(pathname) };

    match util::kompo_path_at(dirfd, raw) {
        Some(resolved) => stat_resolved(&resolved, buf),
        None => unsafe { kompo_wrap::FSTATAT_HANDLE(dirfd, pathname, buf, flags) },
    }
}

#[unsafe(no_mangle)]
pub fn fstat_from_fs(fd: i32, stat: *mut libc::stat) -> i32 {
    if !util::is_fd_exists_in_kompo(fd) {
        return unsafe { kompo_wrap::FSTAT_HANDLE(fd, stat) };
    }

    if stat.is_null() {
        errno::set_errno(errno::Errno(libc::EFAULT));
        return -1;
    }

    match fs().fstat(fd, unsafe { &mut *stat }) {
        Some(_) => 0,
        None => enoent(),
    }
}

#[unsafe(no_mangle)]
pub fn read_from_fs(fd: i32, buf: *mut libc::c_void, count: libc::size_t) -> isize {
    if !util::is_fd_exists_in_kompo(fd) {
        return unsafe { kompo_wrap::READ_HANDLE(fd, buf, count) };
    }

    let buf = unsafe { std::slice::from_raw_parts_mut(buf as *mut u8, count) };

    match fs().read(fd, buf) {
        Some(read_bytes) => read_bytes,
        None => enoent() as isize,
    }
}

#[unsafe(no_mangle)]
pub fn getcwd_from_fs(buf: *mut libc::c_char, count: libc::size_t) -> *const libc::c_char {
    let working_dir = WORKING_DIR.read().unwrap();
    let Some(working_dir) = working_dir.as_deref() else {
        return unsafe { kompo_wrap::GETCWD_HANDLE(buf, count) };
    };

    if !buf.is_null() || count != 0 {
        todo!()
    }

    // The caller frees this, matching getcwd(NULL, 0).
    CString::new(working_dir)
        .expect("working directory contains a null byte")
        .into_raw()
}

#[unsafe(no_mangle)]
pub fn chdir_from_fs(path: *const libc::c_char) -> libc::c_int {
    let raw = unsafe { util::path_bytes(path) };

    let Some(resolved) = util::kompo_path(raw) else {
        let ret = unsafe { kompo_wrap::CHDIR_HANDLE(path) };
        if ret == 0 {
            // We left the image, so relative paths are the real filesystem's again.
            *WORKING_DIR.write().unwrap() = None;
        }
        return ret;
    };

    if !fs().is_dir_exists_from_path(&resolved) {
        return -1;
    }

    *WORKING_DIR.write().unwrap() = Some(resolved.into_owned());

    1 // preserved from the previous implementation; chdir(2) returns 0
}

#[unsafe(no_mangle)]
pub fn fdopendir_from_fs(fd: i32) -> *mut libc::DIR {
    if !util::is_fd_exists_in_kompo(fd) {
        return unsafe { kompo_wrap::FDOPENDIR_HANDLE(fd) };
    }

    match fs().fdopendir(fd) {
        Some(dir) => Box::into_raw(Box::new(dir)) as *mut libc::DIR,
        None => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub fn readdir_from_fs(dir: *mut libc::DIR) -> *mut libc::dirent {
    if !unsafe { util::is_dir_exists_in_kompo(dir) } {
        return unsafe { kompo_wrap::READDIR_HANDLE(dir) };
    }

    // The DIR* stays the caller's until closedir, so borrow it. The entry
    // points into it and is only rewritten by the next readdir -- the same
    // contract as readdir(3).
    let handle = unsafe { &mut *(dir as *mut kompo_tree::FsDir) };

    fs().readdir(handle).unwrap_or(std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub fn closedir_from_fs(dir: *mut libc::DIR) -> i32 {
    if !unsafe { util::is_dir_exists_in_kompo(dir) } {
        return unsafe { kompo_wrap::CLOSEDIR_HANDLE(dir) };
    }

    let handle = unsafe { Box::from_raw(dir as *mut kompo_tree::FsDir) };
    fs().closedir(&handle);

    unsafe { kompo_wrap::CLOSE_HANDLE(handle.fd) }
}

#[unsafe(no_mangle)]
pub fn opendir_from_fs(path: *const libc::c_char) -> *mut libc::DIR {
    let raw = unsafe { util::path_bytes(path) };

    let Some(resolved) = util::kompo_path(raw) else {
        return unsafe { kompo_wrap::OPENDIR_HANDLE(path) };
    };

    match fs().opendir(&resolved) {
        Some(dir) => Box::into_raw(Box::new(dir)) as *mut libc::DIR,
        None => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub fn rewinddir_from_fs(dir: *mut libc::DIR) {
    if !unsafe { util::is_dir_exists_in_kompo(dir) } {
        unsafe { kompo_wrap::REWINDDIR_HANDLE(dir) };
        return;
    }

    let handle = unsafe { &mut *(dir as *mut kompo_tree::FsDir) };
    fs().rewinddir(handle);
}

/// # Safety
/// `path` must be a valid pointer to a null-terminated C string, and
/// `resolved_path` either null or a buffer of at least `PATH_MAX` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn realpath_from_fs(
    path: *const libc::c_char,
    resolved_path: *mut libc::c_char,
) -> *const libc::c_char {
    let raw = unsafe { util::path_bytes(path) };

    let Some(resolved) = util::kompo_path(raw) else {
        return unsafe { kompo_wrap::REALPATH_HANDLE(path, resolved_path) };
    };

    // realpath(3) promises a canonical path. A relative argument was already
    // normalised against the working directory on the way in; an absolute one
    // reaches us spelled however the caller wrote it.
    let canonical = match resolved {
        Cow::Owned(path) => path,
        Cow::Borrowed(path) => util::join_normalized(b"/", path),
    };
    let canonical = CString::new(canonical).expect("path contains a null byte");

    if resolved_path.is_null() {
        // The caller frees this, matching realpath(path, NULL).
        return canonical.into_raw();
    }

    let bytes = canonical.as_bytes_with_nul();
    unsafe {
        std::ptr::copy_nonoverlapping(
            bytes.as_ptr() as *const libc::c_char,
            resolved_path,
            bytes.len(),
        );
    }

    resolved_path
}

/// `mkdir` on a directory that is already in the image succeeds; everything
/// else inside the image fails, since the image is read only.
#[unsafe(no_mangle)]
pub fn mkdir_from_fs(path: *const libc::c_char, mode: libc::mode_t) -> libc::c_int {
    let raw = unsafe { util::path_bytes(path) };

    let Some(resolved) = util::kompo_path(raw) else {
        return unsafe { kompo_wrap::MKDIR_HANDLE(path, mode) };
    };

    if fs().lookup(&resolved).is_some() {
        return 0;
    }

    enoent()
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
    let raw = unsafe { util::path_bytes(path) };

    let Some(resolved) = util::kompo_path(raw) else {
        return unsafe {
            kompo_wrap::GETATTRLIST_HANDLE(path, attr_list, attr_buf, attr_buf_size, options)
        };
    };

    let ret = fs().getattrlist(
        &resolved,
        unsafe { &*(attr_list as *const libc::attrlist) },
        attr_buf,
        attr_buf_size,
    );

    match ret {
        Some(r) => r,
        None => enoent(),
    }
}
