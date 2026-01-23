use paste::paste;

/// Macro to define a syscall hook with HANDLE, extern declaration, and wrapper function.
///
/// Usage:
/// - With return type: `syscall_hook!(open, (path: *const libc::c_char, oflag: libc::c_int) -> libc::c_int);`
/// - Without return type: `syscall_hook!(rewinddir, (dirp: *mut libc::DIR));`
macro_rules! syscall_hook {
    // Pattern with return type
    ($syscall:ident, ($($param:ident: $ty:ty),*) -> $ret:ty) => {
        paste! {
            pub static [<$syscall:upper _HANDLE>]: std::sync::LazyLock<
                unsafe extern "C-unwind" fn($($ty),*) -> $ret,
            > = std::sync::LazyLock::new(|| unsafe {
                let handle = libc::dlsym(libc::RTLD_NEXT, concat!(stringify!($syscall), "\0").as_ptr() as _);
                std::mem::transmute::<*mut libc::c_void, unsafe extern "C-unwind" fn($($ty),*) -> $ret>(handle)
            });

            unsafe extern "C" {
                fn [<$syscall _from_fs>]($($param: $ty),*) -> $ret;
            }

            #[unsafe(no_mangle)]
            unsafe extern "C-unwind" fn $syscall($($param: $ty),*) -> $ret {
                unsafe { [<$syscall _from_fs>]($($param),*) }
            }
        }
    };

    // Pattern without return type (void)
    ($syscall:ident, ($($param:ident: $ty:ty),*)) => {
        paste! {
            pub static [<$syscall:upper _HANDLE>]: std::sync::LazyLock<
                unsafe extern "C-unwind" fn($($ty),*),
            > = std::sync::LazyLock::new(|| unsafe {
                let handle = libc::dlsym(libc::RTLD_NEXT, concat!(stringify!($syscall), "\0").as_ptr() as _);
                std::mem::transmute::<*mut libc::c_void, unsafe extern "C-unwind" fn($($ty),*)>(handle)
            });

            unsafe extern "C" {
                fn [<$syscall _from_fs>]($($param: $ty),*);
            }

            #[unsafe(no_mangle)]
            unsafe extern "C-unwind" fn $syscall($($param: $ty),*) {
                unsafe { [<$syscall _from_fs>]($($param),*) }
            }
        }
    };
}

// =============================================================================
// Syscall hooks using the macro
// =============================================================================

syscall_hook!(open, (path: *const libc::c_char, oflag: libc::c_int, mode: libc::mode_t) -> libc::c_int);
syscall_hook!(openat, (dirfd: libc::c_int, pathname: *const libc::c_char, flags: libc::c_int, mode: libc::mode_t) -> libc::c_int);
syscall_hook!(mmap, (addr: *mut libc::c_void, length: libc::size_t, prot: libc::c_int, flags: libc::c_int, fd: libc::c_int, offset: libc::off_t) -> *mut libc::c_void);
syscall_hook!(read, (fd: libc::c_int, buf: *mut libc::c_void, count: libc::size_t) -> libc::ssize_t);
syscall_hook!(stat, (path: *const libc::c_char, buf: *mut libc::stat) -> libc::c_int);
syscall_hook!(fstat, (fildes: libc::c_int, buf: *mut libc::stat) -> libc::c_int);
syscall_hook!(fstatat, (dirfd: libc::c_int, pathname: *const libc::c_char, buf: *mut libc::stat, flags: libc::c_int) -> libc::c_int);
syscall_hook!(lstat, (path: *const libc::c_char, buf: *mut libc::stat) -> libc::c_int);
syscall_hook!(close, (fd: libc::c_int) -> libc::c_int);
syscall_hook!(getcwd, (buf: *mut libc::c_char, length: libc::size_t) -> *const libc::c_char);
syscall_hook!(opendir, (dirname: *const libc::c_char) -> *mut libc::DIR);
syscall_hook!(fdopendir, (fd: libc::c_int) -> *mut libc::DIR);
syscall_hook!(readdir, (dirp: *mut libc::DIR) -> *mut libc::dirent);
syscall_hook!(rewinddir, (dirp: *mut libc::DIR));
syscall_hook!(mkdir, (path: *const libc::c_char, mode: libc::mode_t) -> libc::c_int);
syscall_hook!(closedir, (dirp: *mut libc::DIR) -> libc::c_int);
syscall_hook!(chdir, (path: *const libc::c_char) -> libc::c_int);
syscall_hook!(realpath, (path: *const libc::c_char, resolved_path: *mut libc::c_char) -> *const libc::c_char);

// getattrlist - macOS only
#[cfg(target_os = "macos")]
#[allow(non_snake_case)]
pub static GETATTRLIST_HANDLE: std::sync::LazyLock<
    unsafe extern "C-unwind" fn(
        path: *const libc::c_char,
        attrList: *mut libc::c_void,
        attrBuf: *mut libc::c_void,
        attrBufSize: libc::size_t,
        options: libc::c_ulong,
    ) -> libc::c_int,
> = std::sync::LazyLock::new(|| unsafe {
    let handle = libc::dlsym(libc::RTLD_NEXT, c"getattrlist".as_ptr() as _);
    std::mem::transmute::<
        *mut libc::c_void,
        unsafe extern "C-unwind" fn(
            path: *const libc::c_char,
            attrList: *mut libc::c_void,
            attrBuf: *mut libc::c_void,
            attrBufSize: libc::size_t,
            options: libc::c_ulong,
        ) -> libc::c_int,
    >(handle)
});

#[cfg(target_os = "macos")]
unsafe extern "C" {
    #[allow(non_snake_case)]
    fn getattrlist_from_fs(
        path: *const libc::c_char,
        attrList: *mut libc::c_void,
        attrBuf: *mut libc::c_void,
        attrBufSize: libc::size_t,
        options: libc::c_ulong,
    ) -> libc::c_int;
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
unsafe extern "C-unwind" fn getattrlist(
    path: *const libc::c_char,
    attr_list: *mut libc::c_void,
    attr_buf: *mut libc::c_void,
    attr_buf_size: libc::size_t,
    options: libc::c_ulong,
) -> libc::c_int {
    unsafe { getattrlist_from_fs(path, attr_list, attr_buf, attr_buf_size, options) }
}
