use std::{
    env,
    ffi::{CStr, CString},
    hash::{DefaultHasher, Hash, Hasher},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    str::FromStr,
};

use crate::{TRIE, WD, WORKING_DIR};

pub fn is_under_kompo_working_dir(other_path: *const libc::c_char) -> bool {
    let wd = unsafe { CStr::from_ptr(&WD) };
    let other_path = unsafe { CStr::from_ptr(other_path) };

    other_path.to_bytes().starts_with(wd.to_bytes())
}

pub fn canonicalize_path(base: &mut PathBuf, join_path: &PathBuf) {
    for comp in join_path.components() {
        match comp {
            std::path::Component::Normal(comp) => {
                base.push(comp);
            }
            std::path::Component::ParentDir => {
                base.pop();
            }
            std::path::Component::RootDir => {
                // do nothing
            }
            std::path::Component::Prefix(_) => todo!(),
            std::path::Component::CurDir => {
                // do nothing
            }
        }
    }
}

pub fn expand_kompo_path(raw_path: *const libc::c_char) -> *const libc::c_char {
    let path = unsafe { CStr::from_ptr(raw_path) };
    let path = PathBuf::from_str(path.to_str().expect("invalid path")).expect("invalid path");

    if path.is_absolute() {
        let path = CString::new(path.to_str().expect("invalid path"))
            .expect("invalid path")
            .into_boxed_c_str();
        let path = Box::into_raw(path);

        return path as *const libc::c_char;
    }

    let wd = unsafe { WORKING_DIR.clone().take().unwrap().into_owned() };
    let mut wd = PathBuf::from(wd);

    canonicalize_path(&mut wd, &path);

    let wd = CString::new(wd.to_str().expect("invalid path"))
        .expect("invalid path")
        .into_boxed_c_str();
    let wd = Box::into_raw(wd);

    wd as *const libc::c_char
}

pub fn current_dir_hash() -> u64 {
    let mut hasher = DefaultHasher::new();
    unsafe { WORKING_DIR.take().unwrap().hash(&mut hasher) };
    hasher.finish()
}

pub fn is_under_kompo_tmp_dir(other_path: *const libc::c_char) -> bool {
    let mut tmpdir = env::temp_dir();
    tmpdir.push(format!("{}", current_dir_hash()));
    let other_path = unsafe { CStr::from_ptr(other_path) };

    other_path
        .to_bytes()
        .starts_with(tmpdir.as_os_str().as_bytes())
}

pub fn is_fd_exists_in_kompo(fd: i32) -> bool {
    if TRIE.get().is_none() {
        return false;
    }

    let trie = std::sync::Arc::clone(&TRIE.get().unwrap());
    {
        let trie = trie.lock().unwrap();

        trie.is_fd_exists(fd)
    }
}

pub fn is_dir_exists_in_kompo(dir: *mut libc::DIR) -> bool {
    if TRIE.get().is_none() {
        return false;
    }

    let dir = unsafe { Box::from_raw(dir as *mut kompo_storage::FsDir) };

    let trie = std::sync::Arc::clone(&TRIE.get().unwrap());
    let bool = {
        let trie = trie.lock().unwrap();

        trie.is_dir_exists(&dir)
    };

    let _ = Box::into_raw(dir);
    bool
}
