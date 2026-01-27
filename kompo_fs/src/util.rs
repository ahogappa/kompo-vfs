use std::{
    env,
    ffi::{CStr, CString},
    hash::{DefaultHasher, Hash, Hasher},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    str::FromStr,
};

use crate::{TRIE, WD, WORKING_DIR};

/// # Safety
/// `other_path` must be a valid pointer to a null-terminated C string.
pub unsafe fn is_under_kompo_working_dir(other_path: *const libc::c_char) -> bool {
    let wd = unsafe { CStr::from_ptr(&WD) };
    let other_path = unsafe { CStr::from_ptr(other_path) };

    other_path.to_bytes().starts_with(wd.to_bytes())
}

pub fn canonicalize_path(base: &mut PathBuf, join_path: &Path) {
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

/// # Safety
/// `raw_path` must be a valid pointer to a null-terminated C string.
pub unsafe fn expand_kompo_path(raw_path: *const libc::c_char) -> *const libc::c_char {
    let path = unsafe { CStr::from_ptr(raw_path) };
    let path = PathBuf::from_str(path.to_str().expect("invalid path")).expect("invalid path");

    if path.is_absolute() {
        let path = CString::new(path.to_str().expect("invalid path"))
            .expect("invalid path")
            .into_boxed_c_str();
        let path = Box::into_raw(path);

        return path as *const libc::c_char;
    }

    let wd = WORKING_DIR.read().unwrap().clone().unwrap();
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
    WORKING_DIR
        .read()
        .unwrap()
        .as_ref()
        .unwrap()
        .hash(&mut hasher);
    hasher.finish()
}

/// # Safety
/// `other_path` must be a valid pointer to a null-terminated C string.
pub unsafe fn is_under_kompo_tmp_dir(other_path: *const libc::c_char) -> bool {
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

    let trie = std::sync::Arc::clone(TRIE.get().unwrap());
    trie.is_fd_exists(fd)
}

/// # Safety
/// `dir` must be a valid pointer to a `FsDir` that was previously allocated by this crate.
pub unsafe fn is_dir_exists_in_kompo(dir: *mut libc::DIR) -> bool {
    if TRIE.get().is_none() {
        return false;
    }

    let dir = unsafe { Box::from_raw(dir as *mut kompo_storage::FsDir) };

    let trie = std::sync::Arc::clone(TRIE.get().unwrap());
    let bool = trie.is_dir_exists(&dir);

    let _ = Box::into_raw(dir);
    bool
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_canonicalize_path_simple() {
        let mut base = PathBuf::from("/home/user");
        let join_path = PathBuf::from("documents");

        canonicalize_path(&mut base, &join_path);

        assert_eq!(base, PathBuf::from("/home/user/documents"));
    }

    #[test]
    fn test_canonicalize_path_with_parent_dir() {
        let mut base = PathBuf::from("/home/user/projects");
        let join_path = PathBuf::from("../documents");

        canonicalize_path(&mut base, &join_path);

        assert_eq!(base, PathBuf::from("/home/user/documents"));
    }

    #[test]
    fn test_canonicalize_path_multiple_parent_dirs() {
        let mut base = PathBuf::from("/home/user/projects/rust");
        let join_path = PathBuf::from("../../documents/work");

        canonicalize_path(&mut base, &join_path);

        assert_eq!(base, PathBuf::from("/home/user/documents/work"));
    }

    #[test]
    fn test_canonicalize_path_with_current_dir() {
        let mut base = PathBuf::from("/home/user");
        let join_path = PathBuf::from("./documents/./work");

        canonicalize_path(&mut base, &join_path);

        assert_eq!(base, PathBuf::from("/home/user/documents/work"));
    }

    #[test]
    fn test_canonicalize_path_complex() {
        let mut base = PathBuf::from("/home/user/projects");
        let join_path = PathBuf::from("./rust/../go/./src");

        canonicalize_path(&mut base, &join_path);

        assert_eq!(base, PathBuf::from("/home/user/projects/go/src"));
    }

    #[test]
    fn test_canonicalize_path_absolute_in_join() {
        let mut base = PathBuf::from("/home/user");
        let join_path = PathBuf::from("/etc/config");

        canonicalize_path(&mut base, &join_path);

        // RootDir component is ignored, so only "etc" and "config" are added
        assert_eq!(base, PathBuf::from("/home/user/etc/config"));
    }

    #[test]
    fn test_canonicalize_path_parent_beyond_root() {
        let mut base = PathBuf::from("/home");
        let join_path = PathBuf::from("../../etc");

        canonicalize_path(&mut base, &join_path);

        // After two parent dirs from /home, we're at / then add etc
        assert_eq!(base, PathBuf::from("/etc"));
    }

    #[test]
    fn test_canonicalize_path_empty_join() {
        let mut base = PathBuf::from("/home/user");
        let join_path = PathBuf::from("");

        canonicalize_path(&mut base, &join_path);

        assert_eq!(base, PathBuf::from("/home/user"));
    }

    #[test]
    fn test_canonicalize_path_only_current_dir() {
        let mut base = PathBuf::from("/home/user");
        let join_path = PathBuf::from(".");

        canonicalize_path(&mut base, &join_path);

        assert_eq!(base, PathBuf::from("/home/user"));
    }

    #[test]
    fn test_canonicalize_path_only_parent_dir() {
        let mut base = PathBuf::from("/home/user/documents");
        let join_path = PathBuf::from("..");

        canonicalize_path(&mut base, &join_path);

        assert_eq!(base, PathBuf::from("/home/user"));
    }

    #[test]
    fn test_canonicalize_path_nested_structure() {
        let mut base = PathBuf::from("/");
        let join_path = PathBuf::from("a/b/c/../d/./e");

        canonicalize_path(&mut base, &join_path);

        assert_eq!(base, PathBuf::from("/a/b/d/e"));
    }
}
