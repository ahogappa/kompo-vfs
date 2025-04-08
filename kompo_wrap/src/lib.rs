// use kompo_fs::*;
use std::{collections::HashMap, ffi::CStr, path};

fn initialize_thread_context(
) -> std::sync::Arc<std::sync::RwLock<std::collections::HashMap<libc::pthread_t, bool>>> {
    let mut thread_context = HashMap::new();
    thread_context.insert(unsafe { libc::pthread_self() }, false);
    std::sync::Arc::new(std::sync::RwLock::new(thread_context))
}

// pthread_create
pub static PTHREAD_CREATE_HANDLE: std::sync::LazyLock<
    unsafe extern "C-unwind" fn(
        *mut libc::pthread_t,
        *const libc::pthread_attr_t,
        *const unsafe extern "C-unwind" fn(*mut libc::c_void) -> *mut libc::c_void,
        *const libc::c_void,
    ) -> libc::c_int,
> = std::sync::LazyLock::new(|| unsafe {
    let handle = libc::dlsym(libc::RTLD_NEXT, b"pthread_create\0".as_ptr() as _);
    std::mem::transmute::<
        *mut libc::c_void,
        unsafe extern "C-unwind" fn(
            *mut libc::pthread_t,
            *const libc::pthread_attr_t,
            *const unsafe extern "C-unwind" fn(*mut libc::c_void) -> *mut libc::c_void,
            *const libc::c_void,
        ) -> libc::c_int,
    >(handle)
});

#[no_mangle]
unsafe extern "C-unwind" fn pthread_create(
    thread: *mut libc::pthread_t,
    attr: *const libc::pthread_attr_t,
    start_routine: *const unsafe extern "C-unwind" fn(*mut libc::c_void) -> *mut libc::c_void,
    arg: *const libc::c_void,
) -> libc::c_int {
    PTHREAD_CREATE_HANDLE(thread, attr, start_routine, arg)
    // let ret = PTHREAD_CREATE_HANDLE(thread, attr, start_routine, arg);
    // let binding = std::sync::Arc::clone(THREAD_CONTEXT.get_or_init(initialize_thread_context));
    // {
    //     let mut binding = binding.write().expect("THREAD_CONTEXT is posioned");
    //     let context = binding
    //         .get(&libc::pthread_self())
    //         .expect("not found thread id in THREAD_CONTEXT")
    //         .clone();
    //     binding.insert(*thread, context);
    // }

    // ret
}

extern "C" {
    fn open_from_fs(
        path: *const libc::c_char,
        oflag: libc::c_int,
        mode: libc::mode_t,
    ) -> libc::c_int;
}

// open
pub static OPEN_HANDLE: std::sync::LazyLock<
    unsafe extern "C-unwind" fn(*const libc::c_char, libc::c_int, libc::mode_t) -> libc::c_int,
> = std::sync::LazyLock::new(|| unsafe {
    let handle = libc::dlsym(libc::RTLD_NEXT, b"open\0".as_ptr() as _);
    std::mem::transmute::<
        *mut libc::c_void,
        unsafe extern "C-unwind" fn(*const libc::c_char, libc::c_int, libc::mode_t) -> libc::c_int,
    >(handle)
});

// const ALLOW_OPEN_PATTARN1: i32 = libc::O_RDONLY | libc::O_NONBLOCK | libc::O_CLOEXEC;
// const ALLOW_OPEN_PATTARN2: i32 = libc::O_RDONLY | libc::O_NONBLOCK;
// const ALLOW_OPEN_PATTARN3: i32 = libc::O_RDONLY | libc::O_CLOEXEC;
// const ALLOW_OPEN_PATTARN4: i32 = libc::O_RDONLY | libc::MS_NOATIME;

#[no_mangle]
unsafe extern "C-unwind" fn open(
    path: *const libc::c_char,
    oflag: libc::c_int,
    mode: libc::mode_t,
) -> libc::c_int {
    // println!(
    //     "rust open: path: {:?}, oflag: {}, mode: {}",
    //     CStr::from_ptr(path),
    //     oflag,
    //     mode
    // );
    open_from_fs(path, oflag, mode)
}

// openat
pub static OPENAT_HANDLE: std::sync::LazyLock<
    unsafe extern "C-unwind" fn(
        libc::c_int,
        *const libc::c_char,
        libc::c_int,
        libc::mode_t,
    ) -> libc::c_int,
> = std::sync::LazyLock::new(|| unsafe {
    let handle = libc::dlsym(libc::RTLD_NEXT, b"openat\0".as_ptr() as _);
    std::mem::transmute::<
        *mut libc::c_void,
        unsafe extern "C-unwind" fn(
            libc::c_int,
            *const libc::c_char,
            libc::c_int,
            libc::mode_t,
        ) -> libc::c_int,
    >(handle)
});

// const ALLOW_OPENAT_PATTARN1: i32 = libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY;

extern "C" {
    fn openat_from_fs(
        dirfd: libc::c_int,
        pathname: *const libc::c_char,
        flags: libc::c_int,
        mode: libc::mode_t,
    ) -> libc::c_int;
}

#[no_mangle]
unsafe extern "C-unwind" fn openat(
    dirfd: libc::c_int,
    pathname: *const libc::c_char,
    flags: libc::c_int,
    mode: libc::mode_t,
) -> libc::c_int {
    // println!(
    //     "rust openat: path: {:?}, fd: {}, flags: {}, mode: {}",
    //     CStr::from_ptr(pathname),
    //     dirfd,
    //     flags,
    //     mode
    // );
    openat_from_fs(dirfd, pathname, flags, mode)
}

// mmap
pub static MMAP_HANDLE: std::sync::LazyLock<
    unsafe extern "C-unwind" fn(
        addr: *mut libc::c_void,
        length: libc::size_t,
        prot: libc::c_int,
        flags: libc::c_int,
        fd: libc::c_int,
        offset: libc::off_t,
    ) -> *mut libc::c_void,
> = std::sync::LazyLock::new(|| unsafe {
    let handle = libc::dlsym(libc::RTLD_NEXT, b"mmap\0".as_ptr() as _);
    std::mem::transmute::<
        *mut libc::c_void,
        unsafe extern "C-unwind" fn(
            addr: *mut libc::c_void,
            length: libc::size_t,
            prot: libc::c_int,
            flags: libc::c_int,
            fd: libc::c_int,
            offset: libc::off_t,
        ) -> *mut libc::c_void,
    >(handle)
});

extern "C" {
    fn mmap_from_fs(
        addr: *mut libc::c_void,
        length: libc::size_t,
        prot: libc::c_int,
        flags: libc::c_int,
        fd: libc::c_int,
        offset: libc::off_t,
    ) -> *mut libc::c_void;
}

#[no_mangle]
unsafe extern "C-unwind" fn mmap(
    addr: *mut libc::c_void,
    length: libc::size_t,
    prot: libc::c_int,
    flags: libc::c_int,
    fd: libc::c_int,
    offset: libc::off_t,
) -> *mut libc::c_void {
    mmap_from_fs(addr, length, prot, flags, fd, offset)
}

// read
pub static READ_HANDLE: std::sync::LazyLock<
    unsafe extern "C-unwind" fn(
        fd: libc::c_int,
        buf: *mut libc::c_void,
        count: libc::size_t,
    ) -> libc::ssize_t,
> = std::sync::LazyLock::new(|| unsafe {
    let handle = libc::dlsym(libc::RTLD_NEXT, b"read\0".as_ptr() as _);
    std::mem::transmute::<
        *mut libc::c_void,
        unsafe extern "C-unwind" fn(
            fd: libc::c_int,
            buf: *mut libc::c_void,
            count: libc::size_t,
        ) -> libc::ssize_t,
    >(handle)
});

extern "C" {
    fn read_from_fs(fd: libc::c_int, buf: *mut libc::c_void, count: libc::size_t) -> libc::ssize_t;
}

#[no_mangle]
unsafe extern "C-unwind" fn read(
    fd: libc::c_int,
    buf: *mut libc::c_void,
    count: libc::size_t,
) -> libc::ssize_t {
    // println!("rust read: fd: {}, count: {}", fd, count);
    read_from_fs(fd, buf, count)
}

// readv
// pub static READV_HANDLE: std::sync::LazyLock<
//     unsafe extern "C-unwind" fn(libc::c_int, *const libc::iovec, libc::c_int) -> libc::ssize_t,
// > = std::sync::LazyLock::new(|| unsafe {
//     let handle = libc::dlsym(libc::RTLD_NEXT, b"readv\0".as_ptr() as _);
//     std::mem::transmute::<
//         *mut libc::c_void,
//         unsafe extern "C-unwind" fn(libc::c_int, *const libc::iovec, libc::c_int) -> libc::ssize_t,
//     >(handle)
// });

// #[no_mangle]
// unsafe extern "C-unwind" fn readv(
//     fd: libc::c_int,
//     iov: *const libc::iovec,
//     iovcnt: libc::c_int,
// ) -> libc::ssize_t {
//     println!("rust readv");

//     READV_HANDLE(fd, iov, iovcnt)
// }

//pread
// pub static PREAD_HANDLE: std::sync::LazyLock<
//     unsafe extern "C-unwind" fn(
//         libc::c_int,
//         *mut libc::c_void,
//         libc::size_t,
//         libc::off_t,
//     ) -> libc::ssize_t,
// > = std::sync::LazyLock::new(|| unsafe {
//     let handle = libc::dlsym(libc::RTLD_NEXT, b"pread\0".as_ptr() as _);
//     std::mem::transmute::<
//         *mut libc::c_void,
//         unsafe extern "C-unwind" fn(
//             libc::c_int,
//             *mut libc::c_void,
//             libc::size_t,
//             libc::off_t,
//         ) -> libc::ssize_t,
//     >(handle)
// });

// #[no_mangle]
// pub unsafe extern "C-unwind" fn pread(
//     fd: libc::c_int,
//     buf: *mut libc::c_void,
//     count: libc::size_t,
//     offset: libc::off_t,
// ) -> libc::ssize_t {
//     println!("rust pread");

//     PREAD_HANDLE(fd, buf, count, offset)
// }

//lseek
// pub static LSEEK_HANDLE: std::sync::LazyLock<
//     unsafe extern "C-unwind" fn(libc::c_int, libc::off_t, libc::c_int) -> libc::off_t,
// > = std::sync::LazyLock::new(|| unsafe {
//     let handle = libc::dlsym(libc::RTLD_NEXT, b"lseek\0".as_ptr() as _);
//     std::mem::transmute::<
//         *mut libc::c_void,
//         unsafe extern "C-unwind" fn(libc::c_int, libc::off_t, libc::c_int) -> libc::off_t,
//     >(handle)
// });

// #[no_mangle]
// unsafe extern "C-unwind" fn lseek(
//     fildes: libc::c_int,
//     offset: libc::off_t,
//     whence: libc::c_int,
// ) -> libc::off_t {
//     LSEEK_HANDLE(fildes, offset, whence)
// }

//stat
pub static STAT_HANDLE: std::sync::LazyLock<
    unsafe extern "C-unwind" fn(*const libc::c_char, *mut libc::stat) -> libc::c_int,
> = std::sync::LazyLock::new(|| unsafe {
    let handle = libc::dlsym(libc::RTLD_NEXT, b"stat\0".as_ptr() as _);
    std::mem::transmute::<
        *mut libc::c_void,
        unsafe extern "C-unwind" fn(*const libc::c_char, *mut libc::stat) -> libc::c_int,
    >(handle)
});

extern "C" {
    fn stat_from_fs(path: *const libc::c_char, buf: *mut libc::stat) -> libc::c_int;
}

#[no_mangle]
unsafe extern "C-unwind" fn stat(path: *const libc::c_char, buf: *mut libc::stat) -> libc::c_int {
    stat_from_fs(path, buf)
}

//fstat
pub static FSTAT_HANDLE: std::sync::LazyLock<
    unsafe extern "C-unwind" fn(fildes: libc::c_int, buf: *mut libc::stat) -> libc::c_int,
> = std::sync::LazyLock::new(|| unsafe {
    let handle = libc::dlsym(libc::RTLD_NEXT, b"fstat\0".as_ptr() as _);
    std::mem::transmute::<
        *mut libc::c_void,
        unsafe extern "C-unwind" fn(fildes: libc::c_int, buf: *mut libc::stat) -> libc::c_int,
    >(handle)
});

extern "C" {
    fn fstat_from_fs(fildes: libc::c_int, buf: *mut libc::stat) -> libc::c_int;
}

#[no_mangle]
unsafe extern "C-unwind" fn fstat(fildes: libc::c_int, buf: *mut libc::stat) -> libc::c_int {
    fstat_from_fs(fildes, buf)
}

//fstatat
pub static FSTATAT_HANDLE: std::sync::LazyLock<
    unsafe extern "C-unwind" fn(
        dirfd: libc::c_int,
        pathname: *const libc::c_char,
        buf: *mut libc::stat,
        flags: libc::c_int,
    ) -> libc::c_int,
> = std::sync::LazyLock::new(|| unsafe {
    let handle = libc::dlsym(libc::RTLD_NEXT, b"fstatat\0".as_ptr() as _);
    std::mem::transmute::<
        *mut libc::c_void,
        unsafe extern "C-unwind" fn(
            dirfd: libc::c_int,
            pathname: *const libc::c_char,
            buf: *mut libc::stat,
            flags: libc::c_int,
        ) -> libc::c_int,
    >(handle)
});

extern "C" {
    fn fstatat_from_fs(
        dirfd: libc::c_int,
        pathname: *const libc::c_char,
        buf: *mut libc::stat,
        flags: libc::c_int,
    ) -> libc::c_int;
}

#[no_mangle]
unsafe extern "C-unwind" fn fstatat(
    dirfd: libc::c_int,
    pathname: *const libc::c_char,
    buf: *mut libc::stat,
    flags: libc::c_int,
) -> libc::c_int {
    fstatat_from_fs(dirfd, pathname, buf, flags)
}

//lstat
pub static LSTAT_HANDLE: std::sync::LazyLock<
    unsafe extern "C-unwind" fn(path: *const libc::c_char, buf: *mut libc::stat) -> libc::c_int,
> = std::sync::LazyLock::new(|| unsafe {
    let handle = libc::dlsym(libc::RTLD_NEXT, b"lstat\0".as_ptr() as _);
    std::mem::transmute::<
        *mut libc::c_void,
        unsafe extern "C-unwind" fn(path: *const libc::c_char, buf: *mut libc::stat) -> libc::c_int,
    >(handle)
});

extern "C" {
    fn lstat_from_fs(path: *const libc::c_char, buf: *mut libc::stat) -> libc::c_int;
}

#[no_mangle]
unsafe extern "C-unwind" fn lstat(path: *const libc::c_char, buf: *mut libc::stat) -> libc::c_int {
    lstat_from_fs(path, buf)
}

extern "C" {
    fn close_from_fs(fd: libc::c_int) -> libc::c_int;
}

//close
pub static CLOSE_HANDLE: std::sync::LazyLock<
    unsafe extern "C-unwind" fn(libc::c_int) -> libc::c_int,
> = std::sync::LazyLock::new(|| unsafe {
    let handle = libc::dlsym(libc::RTLD_NEXT, b"close\0".as_ptr() as _);
    std::mem::transmute::<*mut libc::c_void, unsafe extern "C-unwind" fn(libc::c_int) -> libc::c_int>(
        handle,
    )
});

#[no_mangle]
unsafe extern "C-unwind" fn close(d: libc::c_int) -> libc::c_int {
    close_from_fs(d)
}

//getcwd
pub static GETCWD_HANDLE: std::sync::LazyLock<
    unsafe extern "C-unwind" fn(
        buf: *mut libc::c_char,
        length: libc::size_t,
    ) -> *const libc::c_char,
> = std::sync::LazyLock::new(|| unsafe {
    let handle = libc::dlsym(libc::RTLD_NEXT, b"getcwd\0".as_ptr() as _);
    std::mem::transmute::<
        *mut libc::c_void,
        unsafe extern "C-unwind" fn(
            buf: *mut libc::c_char,
            length: libc::size_t,
        ) -> *const libc::c_char,
    >(handle)
});

extern "C" {
    fn getcwd_from_fs(buf: *mut libc::c_char, length: libc::size_t) -> *const libc::c_char;
}

#[no_mangle]
unsafe extern "C-unwind" fn getcwd(
    buf: *mut libc::c_char,
    length: libc::size_t,
) -> *const libc::c_char {
    // println!("rust getcwd len: {}, buf: {:?}", length, buf);

    getcwd_from_fs(buf, length)
}

//getwd
// pub static GETWD_HANDLE: std::sync::LazyLock<
//     unsafe extern "C-unwind" fn(path_name: *const libc::c_char) -> *const libc::c_char,
// > = std::sync::LazyLock::new(|| unsafe {
//     let handle = libc::dlsym(libc::RTLD_NEXT, b"getwd\0".as_ptr() as _);
//     std::mem::transmute::<
//         *mut libc::c_void,
//         unsafe extern "C-unwind" fn(path_name: *const libc::c_char) -> *const libc::c_char,
//     >(handle)
// });

// #[no_mangle]
// unsafe extern "C-unwind" fn getwd(path_name: *const libc::c_char) -> *const libc::c_char {
//     println!("rust getwd: {:?}", CStr::from_ptr(path_name));

//     GETWD_HANDLE(path_name)
// }

//execv
// pub static EXECV_HANDLE: std::sync::LazyLock<
//     unsafe extern "C-unwind" fn(
//         prog: *const libc::c_char,
//         argv: *const *const libc::c_char,
//     ) -> libc::c_int,
// > = std::sync::LazyLock::new(|| unsafe {
//     let handle = libc::dlsym(libc::RTLD_NEXT, b"execv\0".as_ptr() as _);
//     std::mem::transmute::<
//         *mut libc::c_void,
//         unsafe extern "C-unwind" fn(
//             prog: *const libc::c_char,
//             argv: *const *const libc::c_char,
//         ) -> libc::c_int,
//     >(handle)
// });

// #[no_mangle]
// unsafe extern "C-unwind" fn execv(
//     prog: *const libc::c_char,
//     argv: *const *const libc::c_char,
// ) -> libc::c_int {
//     println!("rust execv");

//     EXECV_HANDLE(prog, argv)
// }

//access
// pub static ACCSESS_HANDLE: std::sync::LazyLock<
//     unsafe extern "C-unwind" fn(path: *const libc::c_char, amode: libc::c_int) -> libc::c_int,
// > = std::sync::LazyLock::new(|| unsafe {
//     let handle = libc::dlsym(libc::RTLD_NEXT, b"access\0".as_ptr() as _);
//     std::mem::transmute::<
//         *mut libc::c_void,
//         unsafe extern "C-unwind" fn(path: *const libc::c_char, amode: libc::c_int) -> libc::c_int,
//     >(handle)
// });

// #[no_mangle]
// unsafe extern "C-unwind" fn access(path: *const libc::c_char, amode: libc::c_int) -> libc::c_int {
//     println!("rust access");

//     ACCSESS_HANDLE(path, amode)
// }

//opendir
pub static OPENDIR_HANDLE: std::sync::LazyLock<
    unsafe extern "C-unwind" fn(dirname: *const libc::c_char) -> *mut libc::DIR,
> = std::sync::LazyLock::new(|| unsafe {
    let handle = libc::dlsym(libc::RTLD_NEXT, b"opendir\0".as_ptr() as _);
    std::mem::transmute::<
        *mut libc::c_void,
        unsafe extern "C-unwind" fn(dirname: *const libc::c_char) -> *mut libc::DIR,
    >(handle)
});

extern "C" {
    fn opendir_from_fs(dirname: *const libc::c_char) -> *mut libc::DIR;
}

#[no_mangle]
unsafe extern "C-unwind" fn opendir(dirname: *const libc::c_char) -> *mut libc::DIR {
    opendir_from_fs(dirname)
}

//fdopendir
pub static FDOPENDIR_HANDLE: std::sync::LazyLock<
    unsafe extern "C-unwind" fn(fd: libc::c_int) -> *mut libc::DIR,
> = std::sync::LazyLock::new(|| unsafe {
    let handle = libc::dlsym(libc::RTLD_NEXT, b"fdopendir\0".as_ptr() as _);
    std::mem::transmute::<
        *mut libc::c_void,
        unsafe extern "C-unwind" fn(fd: libc::c_int) -> *mut libc::DIR,
    >(handle)
});

extern "C" {
    fn fdopendir_from_fs(fd: libc::c_int) -> *mut libc::DIR;
}

#[no_mangle]
unsafe extern "C-unwind" fn fdopendir(fd: libc::c_int) -> *mut libc::DIR {
    // println!("rust fdopendir: {:?}", fd);

    fdopendir_from_fs(fd)
}

//readdir
pub static READDIR_HANDLE: std::sync::LazyLock<
    unsafe extern "C-unwind" fn(dirp: *mut libc::DIR) -> *mut libc::dirent,
> = std::sync::LazyLock::new(|| unsafe {
    let handle = libc::dlsym(libc::RTLD_NEXT, b"readdir\0".as_ptr() as _);
    std::mem::transmute::<
        *mut libc::c_void,
        unsafe extern "C-unwind" fn(dirp: *mut libc::DIR) -> *mut libc::dirent,
    >(handle)
});

extern "C" {
    fn readdir_from_fs(dirp: *mut libc::DIR) -> *mut libc::dirent;
}

#[no_mangle]
unsafe extern "C-unwind" fn readdir(dirp: *mut libc::DIR) -> *mut libc::dirent {
    readdir_from_fs(dirp)
}

//telledir
// pub static TELLDIR_HANDLE: std::sync::LazyLock<
//     unsafe extern "C-unwind" fn(dirp: *mut libc::DIR) -> libc::c_long,
// > = std::sync::LazyLock::new(|| unsafe {
//     let handle = libc::dlsym(libc::RTLD_NEXT, b"telldir\0".as_ptr() as _);
//     std::mem::transmute::<
//         *mut libc::c_void,
//         unsafe extern "C-unwind" fn(dirp: *mut libc::DIR) -> libc::c_long,
//     >(handle)
// });

// #[no_mangle]
// unsafe extern "C-unwind" fn telldir(dirp: *mut libc::DIR) -> libc::c_long {
//     println!("rust telldir: {:?}", dirp);

//     TELLDIR_HANDLE(dirp)
// }

//rewinddir
pub static REWINDDIR_HANDLE: std::sync::LazyLock<
    unsafe extern "C-unwind" fn(dirp: *mut libc::DIR),
> = std::sync::LazyLock::new(|| unsafe {
    let handle = libc::dlsym(libc::RTLD_NEXT, b"rewinddir\0".as_ptr() as _);
    std::mem::transmute::<*mut libc::c_void, unsafe extern "C-unwind" fn(dirp: *mut libc::DIR)>(
        handle,
    )
});

extern "C" {
    fn rewinddir_from_fs(dirp: *mut libc::DIR);
}

#[no_mangle]
unsafe extern "C-unwind" fn rewinddir(dirp: *mut libc::DIR) {
    // println!("rust rewinddir: {:?}", dirp);

    rewinddir_from_fs(dirp)
}

//seekdir
// pub static SEEKDIR_HANDLE: std::sync::LazyLock<
//     unsafe extern "C-unwind" fn(dirp: *mut libc::DIR, loc: libc::c_long),
// > = std::sync::LazyLock::new(|| unsafe {
//     let handle = libc::dlsym(libc::RTLD_NEXT, b"seekdir\0".as_ptr() as _);
//     std::mem::transmute::<
//         *mut libc::c_void,
//         unsafe extern "C-unwind" fn(dirp: *mut libc::DIR, loc: libc::c_long),
//     >(handle)
// });

// #[no_mangle]
// unsafe extern "C-unwind" fn seekdir(dirp: *mut libc::DIR, loc: libc::c_long) {
//     println!("rust seekdir: {:?}", dirp);

//     SEEKDIR_HANDLE(dirp, loc)
// }

//dirfd
// pub static DIRFD_HANDLE: std::sync::LazyLock<
//     unsafe extern "C-unwind" fn(dirp: *mut libc::DIR) -> libc::c_int,
// > = std::sync::LazyLock::new(|| unsafe {
//     let handle = libc::dlsym(libc::RTLD_NEXT, b"dirfd\0".as_ptr() as _);
//     std::mem::transmute::<
//         *mut libc::c_void,
//         unsafe extern "C-unwind" fn(dirp: *mut libc::DIR) -> libc::c_int,
//     >(handle)
// });

// #[no_mangle]
// unsafe extern "C-unwind" fn dirfd(dirp: *mut libc::DIR) -> libc::c_int {
//     println!("rust dirfd: {:?}", dirp);

//     DIRFD_HANDLE(dirp)
// }

//mkdir
pub static MKDIR_HANDLE: std::sync::LazyLock<
    unsafe extern "C-unwind" fn(path: *const libc::c_char, mode: libc::mode_t) -> libc::c_int,
> = std::sync::LazyLock::new(|| unsafe {
    let handle = libc::dlsym(libc::RTLD_NEXT, b"mkdir\0".as_ptr() as _);
    std::mem::transmute::<
        *mut libc::c_void,
        unsafe extern "C-unwind" fn(path: *const libc::c_char, mode: libc::mode_t) -> libc::c_int,
    >(handle)
});

extern "C" {
    fn mkdir_from_fs(path: *const libc::c_char, mode: libc::mode_t) -> libc::c_int;
}

#[no_mangle]
unsafe extern "C-unwind" fn mkdir(path: *const libc::c_char, mode: libc::mode_t) -> libc::c_int {
    // println!("rust mkdir: {:?}", CStr::from_ptr(path));

    mkdir_from_fs(path, mode)
}

//closedir
pub static CLOSEDIR_HANDLE: std::sync::LazyLock<
    unsafe extern "C-unwind" fn(dirp: *mut libc::DIR) -> libc::c_int,
> = std::sync::LazyLock::new(|| unsafe {
    let handle = libc::dlsym(libc::RTLD_NEXT, b"closedir\0".as_ptr() as _);
    std::mem::transmute::<
        *mut libc::c_void,
        unsafe extern "C-unwind" fn(dirp: *mut libc::DIR) -> libc::c_int,
    >(handle)
});

extern "C" {
    fn closedir_from_fs(dirp: *mut libc::DIR) -> libc::c_int;
}

#[no_mangle]
unsafe extern "C-unwind" fn closedir(dirp: *mut libc::DIR) -> libc::c_int {
    // println!("rust closedir: {:?}", dirp);

    closedir_from_fs(dirp)
}

//chdir
pub static CHDIR_HANDLE: std::sync::LazyLock<
    unsafe extern "C-unwind" fn(path: *const libc::c_char) -> libc::c_int,
> = std::sync::LazyLock::new(|| unsafe {
    let handle = libc::dlsym(libc::RTLD_NEXT, b"chdir\0".as_ptr() as _);
    std::mem::transmute::<
        *mut libc::c_void,
        unsafe extern "C-unwind" fn(path: *const libc::c_char) -> libc::c_int,
    >(handle)
});

extern "C" {
    fn chdir_from_fs(path: *const libc::c_char) -> libc::c_int;
}

#[no_mangle]
unsafe extern "C-unwind" fn chdir(path: *const libc::c_char) -> libc::c_int {
    // println!("rust chdir: {:?}", CStr::from_ptr(path));

    chdir_from_fs(path)
}

//readlink
// pub static READLINK_HANDLE: std::sync::LazyLock<
//     unsafe extern "C-unwind" fn(
//         path: *const libc::c_char,
//         buf: *mut libc::c_char,
//         bufsz: libc::size_t,
//     ) -> libc::ssize_t,
// > = std::sync::LazyLock::new(|| unsafe {
//     let handle = libc::dlsym(libc::RTLD_NEXT, b"readlink\0".as_ptr() as _);
//     std::mem::transmute::<
//         *mut libc::c_void,
//         unsafe extern "C-unwind" fn(
//             path: *const libc::c_char,
//             buf: *mut libc::c_char,
//             bufsz: libc::size_t,
//         ) -> libc::ssize_t,
//     >(handle)
// });

// #[no_mangle]
// unsafe extern "C-unwind" fn readlink(
//     path: *const libc::c_char,
//     buf: *mut libc::c_char,
//     bufsz: libc::size_t,
// ) -> libc::ssize_t {
//     println!("rust readlink: {:?}", CStr::from_ptr(path));

//     READLINK_HANDLE(path, buf, bufsz)
// }

//realpath
pub static REALPATH_HANDLE: std::sync::LazyLock<
    unsafe extern "C-unwind" fn(
        path: *const libc::c_char,
        resolved_path: *mut libc::c_char,
    ) -> *const libc::c_char,
> = std::sync::LazyLock::new(|| unsafe {
    let handle = libc::dlsym(libc::RTLD_NEXT, b"realpath\0".as_ptr() as _);
    std::mem::transmute::<
        *mut libc::c_void,
        unsafe extern "C-unwind" fn(
            path: *const libc::c_char,
            resolved_path: *mut libc::c_char,
        ) -> *const libc::c_char,
    >(handle)
});

extern "C" {
    fn realpath_from_fs(
        path: *const libc::c_char,
        resolved_path: *mut libc::c_char,
    ) -> *const libc::c_char;
}

#[no_mangle]
unsafe extern "C-unwind" fn realpath(
    path: *const libc::c_char,
    resolved_path: *mut libc::c_char,
) -> *const libc::c_char {
    realpath_from_fs(path, resolved_path)
}

// //dlopen
// pub static DLOPEN_HANDLE: std::sync::LazyLock<
//     unsafe extern "C-unwind" fn(
//         filename: *const libc::c_char,
//         flag: libc::c_int,
//     ) -> *mut libc::c_void,
// > = std::sync::LazyLock::new(|| unsafe {
//     let handle = libc::dlsym(libc::RTLD_NEXT, b"dlopen\0".as_ptr() as _);
//     std::mem::transmute::<
//         *mut libc::c_void,
//         unsafe extern "C-unwind" fn(
//             filename: *const libc::c_char,
//             flag: libc::c_int,
//         ) -> *mut libc::c_void,
//     >(handle)
// });

// #[no_mangle]
// unsafe extern "C-unwind" fn dlopen(
//     filename: *const libc::c_char,
//     flag: libc::c_int,
// ) -> *mut libc::c_void {
//     println!("rust dlopen: {:?}", CStr::from_ptr(filename));

//     DLOPEN_HANDLE(filename, flag)
// }
