// This crate provides test data symbols for kompo_fs tests.
// The symbols are defined in dummy_fs.c and compiled by build.rs.

#[allow(dead_code)]
extern "C" {
    pub static PATHS: libc::c_char;
    pub static PATHS_SIZE: libc::c_int;
    pub static FILES: libc::c_char;
    pub static FILES_SIZE: libc::c_int;
    pub static FILES_SIZES: libc::c_ulonglong;
    pub static WD: libc::c_char;
}
