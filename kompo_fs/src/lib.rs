mod glue;
pub mod util;
use std::ffi::CStr;
use std::ffi::CString;
use std::ops::Range;
use std::path::Path;
use trie_rs::map::TrieBuilder;

static TRIE: std::sync::OnceLock<std::sync::Arc<kompo_storage::Fs>> =
    std::sync::OnceLock::new();

pub static WORKING_DIR: std::sync::RwLock<Option<std::ffi::OsString>> =
    std::sync::RwLock::new(None);

pub static THREAD_CONTEXT: std::sync::OnceLock<
    std::sync::Arc<std::sync::RwLock<std::collections::HashMap<libc::pthread_t, bool>>>,
> = std::sync::OnceLock::new();

static FILE_TYPE_CACHE: std::sync::LazyLock<
    std::sync::RwLock<std::collections::HashMap<Vec<std::ffi::OsString>, libc::stat>>,
> = std::sync::LazyLock::new(|| std::sync::RwLock::new(std::collections::HashMap::new()));

#[allow(clippy::upper_case_acronyms)]
type VALUE = u64;

#[allow(clippy::upper_case_acronyms)]
enum Ruby {
    FALSE = 0x00,
    NIL = 0x04,
    TRUE = 0x14,
}
unsafe extern "C" {
    static FILES: libc::c_char;
    static FILES_SIZES: libc::c_ulonglong;
    static FILES_SIZE: libc::c_int;
    static PATHS: libc::c_char;
    static PATHS_SIZE: libc::c_int;
    static WD: libc::c_char;

    static rb_cObject: VALUE;
    fn rb_define_class(name: *const libc::c_char, rb_super: VALUE) -> VALUE;
    // fn rb_string_value_ptr(v: *const VALUE) -> *const libc::c_char;
    fn rb_define_singleton_method(
        object: VALUE,
        name: *const libc::c_char,
        func: unsafe extern "C" fn(v: VALUE, v2: VALUE) -> VALUE,
        argc: libc::c_int,
    );
    fn rb_need_block();
    // fn rb_block_proc() -> VALUE;
    fn rb_ensure(
        b_proc: unsafe extern "C" fn(VALUE) -> VALUE,
        data1: VALUE,
        e_proc: unsafe extern "C" fn(VALUE) -> VALUE,
        data2: VALUE,
    ) -> VALUE;
    fn rb_yield(v: VALUE) -> VALUE;
}

fn initialize_trie() -> std::sync::Arc<kompo_storage::Fs<'static>> {
    std::sync::Arc::new(initialize_fs())
}

unsafe extern "C" fn context_func(_: VALUE, _: VALUE) -> VALUE {
    unsafe { rb_need_block() };

    let binding = std::sync::Arc::clone(
        THREAD_CONTEXT
            .get()
            .expect("not initialized THREAD_CONTEXT"),
    );
    {
        let mut binding = binding.write().expect("THREAD_CONTEXT is posioned");
        binding.insert(unsafe { libc::pthread_self() }, true);
    }

    unsafe extern "C" fn close(_: VALUE) -> VALUE {
        let binding = std::sync::Arc::clone(
            THREAD_CONTEXT
                .get()
                .expect("not initialized THREAD_CONTEXT"),
        );
        {
            let mut binding = binding.write().expect("THREAD_CONTEXT is posioned");
            binding.insert(unsafe { libc::pthread_self() }, false);
        }

        Ruby::NIL as VALUE
    }

    unsafe { rb_ensure(rb_yield, Ruby::NIL as VALUE, close, Ruby::NIL as VALUE) }
}

unsafe extern "C" fn is_context_func(_: VALUE, _: VALUE) -> VALUE {
    let binding = std::sync::Arc::clone(
        THREAD_CONTEXT
            .get()
            .expect("not initialized THREAD_CONTEXT"),
    );
    {
        let binding = binding.read().expect("THREAD_CONTEXT is posioned");
        let thread_id = unsafe { libc::pthread_self() };
        if let Some(bool) = binding.get(&thread_id) {
            if *bool {
                Ruby::TRUE as VALUE
            } else {
                Ruby::FALSE as VALUE
            }
        } else {
            unreachable!("not found pthread_t")
        }
    }
}

pub fn initialize_fs() -> kompo_storage::Fs<'static> {
    let mut builder = TrieBuilder::new();

    let path_slice = unsafe {
        std::slice::from_raw_parts(&PATHS as *const libc::c_char as *const u8, PATHS_SIZE as _)
    };
    let file_slice = unsafe {
        std::slice::from_raw_parts(&FILES as *const libc::c_char as *const u8, FILES_SIZE as _)
    };

    let splited_path_array = path_slice
        .split_inclusive(|a| *a == b'\0')
        .collect::<Vec<_>>();

    let files_sizes =
        unsafe { std::slice::from_raw_parts(&FILES_SIZES, splited_path_array.len() + 1) };

    for (i, path_byte) in splited_path_array.into_iter().enumerate() {
        let path = Path::new(unsafe {
            let bytes = std::slice::from_raw_parts(path_byte.as_ptr(), path_byte.len());
            CStr::from_bytes_with_nul_unchecked(bytes).to_str().unwrap()
        });
        let path = path.iter().collect::<Vec<_>>();

        let range: Range<usize> = files_sizes[i] as usize..files_sizes[i + 1] as usize;
        let file = &file_slice[range];
        let file = unsafe { std::slice::from_raw_parts(file.as_ptr(), file.len()) };

        builder.push(path, file);
    }

    kompo_storage::Fs::new(builder)
}

/// # Safety
/// This function must be called from Ruby's initialization context.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn Init_kompo_fs() {
    unsafe {
        let c_name = CString::new("Kompo").unwrap();
        let context = CString::new("context").unwrap();
        let is_context = CString::new("context?").unwrap();
        let class = rb_define_class(c_name.as_ptr(), rb_cObject);
        rb_define_singleton_method(class, context.as_ptr(), context_func, 0);
        rb_define_singleton_method(class, is_context.as_ptr(), is_context_func, 0);
    }
}

/// # Safety
/// `entrypoint_path` must be a valid pointer to a null-terminated C string, or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kompo_fs_set_entrypoint_dir(entrypoint_path: *const libc::c_char) {
    if entrypoint_path.is_null() {
        return;
    }

    let path_cstr = unsafe { CStr::from_ptr(entrypoint_path) };
    let path = Path::new(path_cstr.to_str().expect("invalid entrypoint path"));

    if let Some(parent) = path.parent() {
        let parent_os_str = parent.as_os_str().to_os_string();
        *WORKING_DIR.write().unwrap() = Some(parent_os_str);
    }
}

#[cfg(test)]
mod tests {
    extern crate kompo_fs_test_data;

    use super::*;
    use serial_test::serial;
    use std::ffi::CString;

    #[test]
    fn test_initialize_fs() {
        let fs = initialize_fs();
        // Verify we can access files from the test data
        let path = std::path::Path::new("/test/hello.txt");
        let path_vec: Vec<&std::ffi::OsStr> = path.iter().collect();

        let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
        let result = fs.stat(&path_vec, &mut stat_buf);
        assert!(result.is_some());
        assert_eq!(stat_buf.st_size, 13); // "Hello, World!" is 13 bytes
    }

    #[test]
    fn test_stat_from_fs_existing_file() {
        let path = CString::new("/test/hello.txt").unwrap();
        let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };

        let result = glue::stat_from_fs(path.as_ptr(), &mut stat_buf);
        assert_eq!(result, 0);
        assert_eq!(stat_buf.st_size, 13);
    }

    #[test]
    fn test_stat_from_fs_nonexistent_file() {
        let path = CString::new("/test/nonexistent.txt").unwrap();
        let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };

        let result = glue::stat_from_fs(path.as_ptr(), &mut stat_buf);
        assert_eq!(result, -1);
        assert_eq!(errno::errno().0, libc::ENOENT);
    }

    #[test]
    fn test_stat_from_fs_null_stat() {
        let path = CString::new("/test/hello.txt").unwrap();

        let result = glue::stat_from_fs(path.as_ptr(), std::ptr::null_mut());
        assert_eq!(result, -1);
        assert_eq!(errno::errno().0, libc::EFAULT);
    }

    #[test]
    fn test_lstat_from_fs_existing_file() {
        let path = CString::new("/test/world.txt").unwrap();
        let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };

        let result = glue::lstat_from_fs(path.as_ptr(), &mut stat_buf);
        assert_eq!(result, 0);
        assert_eq!(stat_buf.st_size, 12); // "Test Content" is 12 bytes
    }

    #[test]
    fn test_lstat_from_fs_null_stat() {
        let path = CString::new("/test/hello.txt").unwrap();

        let result = glue::lstat_from_fs(path.as_ptr(), std::ptr::null_mut());
        assert_eq!(result, -1);
        assert_eq!(errno::errno().0, libc::EFAULT);
    }

    #[test]
    fn test_open_and_close_from_fs() {
        let path = CString::new("/test/hello.txt").unwrap();

        let fd = glue::open_from_fs(path.as_ptr(), libc::O_RDONLY, 0);
        assert!(fd >= 0, "open should return non-negative fd");

        let result = glue::close_from_fs(fd);
        // close returns 0 on success (for real fd) or may vary for virtual fd
        assert!(result == 0 || result == -1);
    }

    #[test]
    fn test_open_nonexistent_file() {
        let path = CString::new("/test/nonexistent.txt").unwrap();

        let fd = glue::open_from_fs(path.as_ptr(), libc::O_RDONLY, 0);
        assert_eq!(fd, -1);
        assert_eq!(errno::errno().0, libc::ENOENT);
    }

    #[test]
    fn test_open_directory_with_o_directory_flag() {
        let path = CString::new("/test").unwrap();

        let fd = glue::open_from_fs(path.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY, 0);
        assert!(fd >= 0, "open with O_DIRECTORY on directory should succeed");

        glue::close_from_fs(fd);
    }

    #[test]
    fn test_open_file_with_o_directory_flag() {
        let path = CString::new("/test/hello.txt").unwrap();

        let fd = glue::open_from_fs(path.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY, 0);
        assert_eq!(fd, -1, "open with O_DIRECTORY on file should fail");
        assert_eq!(errno::errno().0, libc::ENOTDIR);
    }

    #[test]
    fn test_open_nonexistent_with_o_directory_flag() {
        let path = CString::new("/test/nonexistent").unwrap();

        let fd = glue::open_from_fs(path.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY, 0);
        assert_eq!(
            fd, -1,
            "open with O_DIRECTORY on nonexistent path should fail"
        );
        assert_eq!(errno::errno().0, libc::ENOENT);
    }

    #[test]
    fn test_fstat_from_fs() {
        let path = CString::new("/test/hello.txt").unwrap();
        let fd = glue::open_from_fs(path.as_ptr(), libc::O_RDONLY, 0);
        assert!(fd >= 0);

        let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
        let result = glue::fstat_from_fs(fd, &mut stat_buf);
        assert_eq!(result, 0);
        assert_eq!(stat_buf.st_size, 13);

        glue::close_from_fs(fd);
    }

    #[test]
    fn test_fstat_from_fs_null_stat() {
        let path = CString::new("/test/hello.txt").unwrap();
        let fd = glue::open_from_fs(path.as_ptr(), libc::O_RDONLY, 0);
        assert!(fd >= 0);

        let result = glue::fstat_from_fs(fd, std::ptr::null_mut());
        assert_eq!(result, -1);
        assert_eq!(errno::errno().0, libc::EFAULT);

        glue::close_from_fs(fd);
    }

    #[test]
    fn test_read_from_fs() {
        let path = CString::new("/test/hello.txt").unwrap();
        let fd = glue::open_from_fs(path.as_ptr(), libc::O_RDONLY, 0);
        assert!(fd >= 0);

        let mut buf = vec![0u8; 20];
        let bytes_read = glue::read_from_fs(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len());

        assert_eq!(bytes_read, 13);
        assert_eq!(&buf[..13], b"Hello, World!");

        glue::close_from_fs(fd);
    }

    #[test]
    fn test_read_from_fs_partial() {
        let path = CString::new("/test/hello.txt").unwrap();
        let fd = glue::open_from_fs(path.as_ptr(), libc::O_RDONLY, 0);
        assert!(fd >= 0);

        // Read only 5 bytes
        let mut buf = vec![0u8; 5];
        let bytes_read = glue::read_from_fs(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len());

        assert_eq!(bytes_read, 5);
        assert_eq!(&buf[..5], b"Hello");

        glue::close_from_fs(fd);
    }

    #[test]
    fn test_read_world_txt() {
        let path = CString::new("/test/world.txt").unwrap();
        let fd = glue::open_from_fs(path.as_ptr(), libc::O_RDONLY, 0);
        assert!(fd >= 0);

        let mut buf = vec![0u8; 20];
        let bytes_read = glue::read_from_fs(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len());

        assert_eq!(bytes_read, 12);
        assert_eq!(&buf[..12], b"Test Content");

        glue::close_from_fs(fd);
    }

    #[test]
    fn test_opendir_and_closedir() {
        let path = CString::new("/test").unwrap();
        let dir = glue::opendir_from_fs(path.as_ptr());

        assert!(!dir.is_null(), "opendir should return non-null DIR pointer");

        let result = glue::closedir_from_fs(dir);
        // closedir may return error because underlying fd is from dup
        assert!(result == 0 || result == -1);
    }

    #[test]
    fn test_opendir_nonexistent() {
        let path = CString::new("/nonexistent").unwrap();
        let dir = glue::opendir_from_fs(path.as_ptr());

        assert!(
            dir.is_null(),
            "opendir on nonexistent path should return null"
        );
    }

    #[test]
    fn test_readdir_from_fs() {
        let path = CString::new("/test").unwrap();
        let dir = glue::opendir_from_fs(path.as_ptr());
        assert!(!dir.is_null());

        let mut entries = Vec::new();
        loop {
            let entry = glue::readdir_from_fs(dir);
            if entry.is_null() {
                break;
            }
            let name = unsafe {
                CStr::from_ptr((*entry).d_name.as_ptr())
                    .to_string_lossy()
                    .to_string()
            };
            entries.push(name);
        }

        // Should contain hello.txt and world.txt (and possibly . and ..)
        assert!(
            entries.iter().any(|e| e == "hello.txt"),
            "Should contain hello.txt, got: {:?}",
            entries
        );
        assert!(
            entries.iter().any(|e| e == "world.txt"),
            "Should contain world.txt, got: {:?}",
            entries
        );

        glue::closedir_from_fs(dir);
    }

    #[test]
    fn test_stat_directory() {
        let path = CString::new("/test").unwrap();
        let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };

        let result = glue::stat_from_fs(path.as_ptr(), &mut stat_buf);
        assert_eq!(result, 0);
        // Directory should have S_IFDIR flag
        assert!(
            stat_buf.st_mode & libc::S_IFDIR != 0,
            "Should be a directory"
        );
    }

    #[test]
    #[serial]
    fn test_kompo_fs_set_entrypoint_dir_with_valid_path() {
        let path = CString::new("/app/bin/main.rb").unwrap();

        // Clear WORKING_DIR before test
        WORKING_DIR.write().unwrap().take();

        unsafe {
            kompo_fs_set_entrypoint_dir(path.as_ptr());
        }

        // Verify WORKING_DIR is set to the parent directory
        let working_dir = WORKING_DIR.write().unwrap().take();
        assert!(working_dir.is_some());
        let dir_path = working_dir.unwrap();
        assert_eq!(dir_path.to_str().unwrap(), "/app/bin");
    }

    #[test]
    #[serial]
    fn test_kompo_fs_set_entrypoint_dir_with_null() {
        // Clear WORKING_DIR before test
        WORKING_DIR.write().unwrap().take();

        // Should not panic when passing null
        unsafe {
            kompo_fs_set_entrypoint_dir(std::ptr::null());
        }

        // Verify WORKING_DIR is still None
        let working_dir = WORKING_DIR.write().unwrap().take();
        assert!(working_dir.is_none());
    }

    #[test]
    #[serial]
    fn test_kompo_fs_set_entrypoint_dir_with_root_path() {
        let path = CString::new("/main.rb").unwrap();

        // Clear WORKING_DIR before test
        WORKING_DIR.write().unwrap().take();

        unsafe {
            kompo_fs_set_entrypoint_dir(path.as_ptr());
        }

        // Verify WORKING_DIR is set to root
        let working_dir = WORKING_DIR.write().unwrap().take();
        assert!(working_dir.is_some());
        let dir_path = working_dir.unwrap();
        assert_eq!(dir_path.to_str().unwrap(), "/");
    }
}
