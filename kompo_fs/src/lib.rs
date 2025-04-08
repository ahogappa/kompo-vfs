mod glue;
pub mod util;
use std::collections::HashMap;
use std::ffi::CStr;
use std::ffi::CString;
use std::ops::Range;
use std::path::Path;
use trie_rs::map::TrieBuilder;

static TRIE: std::sync::OnceLock<std::sync::Arc<std::sync::Mutex<kompo_storage::Fs>>> =
    std::sync::OnceLock::new();

pub static mut WORKING_DIR: std::cell::RefCell<Option<std::borrow::Cow<'static, std::ffi::OsStr>>> =
    std::cell::RefCell::new(None);

static FD_TABLE: std::sync::LazyLock<std::sync::Arc<std::sync::RwLock<HashMap<i32, Vec<u8>>>>> =
    std::sync::LazyLock::new(|| std::sync::Arc::new(std::sync::RwLock::new(HashMap::new())));

pub static mut THREAD_CONTEXT: std::sync::OnceLock<
    std::sync::Arc<std::sync::RwLock<std::collections::HashMap<libc::pthread_t, bool>>>,
> = std::sync::OnceLock::new();

static mut FILE_TYPE_CACHE: std::cell::LazyCell<
    std::sync::RwLock<std::collections::HashMap<Vec<std::ffi::OsString>, libc::stat>>,
> = std::cell::LazyCell::new(|| std::sync::RwLock::new(std::collections::HashMap::new()));

type VALUE = u64;
enum Ruby {
    FALSE = 0x00,
    NIL = 0x04,
    TRUE = 0x14,
}
extern "C" {
    static FILES: libc::c_char;
    static FILES_SIZES: libc::c_ulonglong;
    static FILES_SIZE: libc::c_int;
    static PATHS: libc::c_char;
    static PATHS_SIZE: libc::c_int;
    static WD: libc::c_char;
    static START_FILE_PATH: libc::c_char;

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

#[no_mangle]
pub unsafe extern "C-unwind" fn get_start_file() -> *const libc::c_char {
    // let path = util::raw_path_to_kompo_path(&START_FILE_PATH);
    // let path = path
    //     .iter()
    //     .map(|os_str| os_str.as_os_str())
    //     .collect::<Vec<_>>();

    // let trie = std::sync::Arc::clone(&TRIE.get_or_init(initialize_trie));
    // {
    //     let trie = trie.lock().expect("trie is poisoned");

    //     trie.file_read(&path).expect("Not fund start file")
    // }
    0 as *const libc::c_char
}

#[no_mangle]
pub unsafe extern "C-unwind" fn get_start_file_name() -> *const libc::c_char {
    std::ffi::CStr::from_ptr(&START_FILE_PATH).as_ptr()
}

fn initialize_trie() -> std::sync::Arc<std::sync::Mutex<kompo_storage::Fs<'static>>> {
    std::sync::Arc::new(std::sync::Mutex::new(initialize_fs()))
}

unsafe extern "C" fn context_func(_: VALUE, _: VALUE) -> VALUE {
    rb_need_block();

    let binding = std::sync::Arc::clone(
        THREAD_CONTEXT
            .get()
            .expect("not initialized THREAD_CONTEXT"),
    );
    {
        let mut binding = binding.write().expect("THREAD_CONTEXT is posioned");
        binding.insert(libc::pthread_self(), true);
    }

    unsafe extern "C" fn close(_: VALUE) -> VALUE {
        let binding = std::sync::Arc::clone(
            THREAD_CONTEXT
                .get()
                .expect("not initialized THREAD_CONTEXT"),
        );
        {
            let mut binding = binding.write().expect("THREAD_CONTEXT is posioned");
            binding.insert(libc::pthread_self(), false);
        }

        Ruby::NIL as VALUE
    }

    return rb_ensure(rb_yield, Ruby::NIL as VALUE, close, Ruby::NIL as VALUE);
}

unsafe extern "C" fn is_context_func(_: VALUE, _: VALUE) -> VALUE {
    let binding = std::sync::Arc::clone(
        THREAD_CONTEXT
            .get()
            .expect("not initialized THREAD_CONTEXT"),
    );
    {
        let binding = binding.read().expect("THREAD_CONTEXT is posioned");
        if let Some(bool) = binding.get(&libc::pthread_self()) {
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

    let path_slice = unsafe { std::slice::from_raw_parts(&PATHS, PATHS_SIZE as _) };
    let file_slice = unsafe { std::slice::from_raw_parts(&FILES, FILES_SIZE as _) };

    let splited_path_array = path_slice
        .split_inclusive(|a| *a == b'\0')
        .collect::<Vec<_>>();

    let files_sizes =
        unsafe { std::slice::from_raw_parts(&FILES_SIZES, splited_path_array.len() + 1) };

    for (i, path_byte) in splited_path_array.into_iter().enumerate() {
        let path = Path::new(unsafe {
            CStr::from_bytes_with_nul_unchecked(path_byte)
                .to_str()
                .unwrap()
        });
        let path = path.iter().collect::<Vec<_>>();

        let range: Range<usize> = files_sizes[i] as usize..files_sizes[i + 1] as usize;
        let file = &file_slice[range];

        builder.push(path, file);
    }

    kompo_storage::Fs::new(builder)
}

#[no_mangle]
pub unsafe extern "C-unwind" fn Init_kompo_fs() {
    let c_name = CString::new("Kompo").unwrap();
    let context = CString::new("context").unwrap();
    let is_context = CString::new("context?").unwrap();
    let class = rb_define_class(c_name.as_ptr(), rb_cObject);
    rb_define_singleton_method(class, context.as_ptr(), context_func, 0);
    rb_define_singleton_method(class, is_context.as_ptr(), is_context_func, 0);
}
