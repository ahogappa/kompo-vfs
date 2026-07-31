use std::borrow::Cow;
use std::ffi::CStr;
use std::sync::OnceLock;

use crate::{FS, WD, WORKING_DIR};

/// Is `path` inside the packed working directory?
///
/// The generator emits `WD` canonical and without a trailing slash, so a byte
/// prefix match answers it. The boundary check is what keeps a sibling like
/// `/tmp/kompo-abcdefg` out when the working directory is `/tmp/kompo-abcdef`.
pub fn is_under_kompo_working_dir(path: &[u8]) -> bool {
    let wd = working_dir_prefix();

    path.starts_with(wd) && matches!(path.get(wd.len()), None | Some(b'/'))
}

/// The packed working directory, measured once.
///
/// `WD` is a linker constant, so the `strlen` behind `CStr::from_ptr` finds the
/// same answer every time -- and this runs on the reject path of every
/// intercepted call, before we know the path is not ours.
fn working_dir_prefix() -> &'static [u8] {
    static PREFIX: OnceLock<&'static [u8]> = OnceLock::new();

    PREFIX.get_or_init(|| unsafe { CStr::from_ptr(&raw const WD) }.to_bytes())
}

/// Index just past the parent directory of `path`, clamped so it never points
/// above the root, or `None` when there is no separator to split on.
pub fn parent_end(path: &[u8]) -> Option<usize> {
    path.iter()
        .rposition(|&b| b == b'/')
        .map(|slash| slash.max(1))
}

/// Join `rel` onto `base`, resolving `.` and `..` lexically.
///
/// Nothing here touches the real filesystem, so it cannot follow a symlink the
/// way `realpath(3)` would. The image holds no symlinks, so there is nothing to
/// follow.
pub fn join_normalized(base: &[u8], rel: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(base.len() + 1 + rel.len());
    out.extend_from_slice(base);

    for comp in rel.split(|&b| b == b'/') {
        match comp {
            b"" | b"." => {}
            b".." => {
                if let Some(end) = parent_end(&out) {
                    out.truncate(end);
                }
            }
            _ => {
                if out.last() != Some(&b'/') {
                    out.push(b'/');
                }
                out.extend_from_slice(comp);
            }
        }
    }

    out
}

/// The absolute path this call should resolve inside the image, or `None` when
/// it belongs to the real filesystem.
///
/// An absolute path is borrowed as-is; a relative one is joined onto the
/// working directory, which only means anything while that directory is itself
/// inside the image.
pub fn kompo_path(path: &[u8]) -> Option<Cow<'_, [u8]>> {
    if path.first() == Some(&b'/') {
        return is_under_kompo_working_dir(path).then_some(Cow::Borrowed(path));
    }

    let working_dir = WORKING_DIR.read().ok()?;
    let working_dir = working_dir.as_deref()?;

    Some(Cow::Owned(join_normalized(working_dir, path)))
}

/// Like [`kompo_path`], for the `*at` calls.
///
/// A relative name is only ours when it resolves against the working
/// directory, which is what `AT_FDCWD` asks for; any other `dirfd` names a
/// directory in the real filesystem.
pub fn kompo_path_at(dirfd: libc::c_int, path: &[u8]) -> Option<Cow<'_, [u8]>> {
    if path.first() != Some(&b'/') && dirfd != libc::AT_FDCWD {
        return None;
    }

    kompo_path(path)
}

/// # Safety
/// `path` must be a valid pointer to a null-terminated C string.
pub unsafe fn path_bytes<'a>(path: *const libc::c_char) -> &'a [u8] {
    unsafe { CStr::from_ptr(path) }.to_bytes()
}

pub fn is_fd_exists_in_kompo(fd: i32) -> bool {
    FS.get().is_some_and(|fs| fs.is_fd_exists(fd))
}

/// # Safety
/// `dir` must be a valid pointer to an `FsDir` that was previously allocated by
/// this crate.
pub unsafe fn is_dir_exists_in_kompo(dir: *mut libc::DIR) -> bool {
    let Some(fs) = FS.get() else {
        return false;
    };
    if dir.is_null() {
        return false;
    }

    let dir = unsafe { &*(dir as *const kompo_tree::FsDir) };

    fs.is_dir_exists(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_normalized_appends_components() {
        assert_eq!(
            join_normalized(b"/home/user", b"documents"),
            b"/home/user/documents"
        );
        assert_eq!(
            join_normalized(b"/home/user", b"documents/work"),
            b"/home/user/documents/work"
        );
    }

    #[test]
    fn join_normalized_resolves_parent_components() {
        assert_eq!(
            join_normalized(b"/home/user/projects", b"../documents"),
            b"/home/user/documents"
        );
        assert_eq!(
            join_normalized(b"/home/user/projects/rust", b"../../documents/work"),
            b"/home/user/documents/work"
        );
        assert_eq!(
            join_normalized(b"/home/user/documents", b".."),
            b"/home/user"
        );
    }

    #[test]
    fn join_normalized_drops_current_dir_components() {
        assert_eq!(
            join_normalized(b"/home/user", b"./documents/./work"),
            b"/home/user/documents/work"
        );
        assert_eq!(join_normalized(b"/home/user", b"."), b"/home/user");
        assert_eq!(join_normalized(b"/home/user", b""), b"/home/user");
    }

    #[test]
    fn join_normalized_handles_mixed_components() {
        assert_eq!(
            join_normalized(b"/home/user/projects", b"./rust/../go/./src"),
            b"/home/user/projects/go/src"
        );
        assert_eq!(join_normalized(b"/", b"a/b/c/../d/./e"), b"/a/b/d/e");
    }

    #[test]
    fn join_normalized_never_escapes_the_root() {
        assert_eq!(join_normalized(b"/home", b"../../etc"), b"/etc");
        assert_eq!(join_normalized(b"/", b".."), b"/");
    }

    /// A leading separator in the joined path is a component boundary, not a
    /// restart -- this matches how the previous `PathBuf`-based version behaved.
    #[test]
    fn join_normalized_treats_a_rooted_join_as_relative() {
        assert_eq!(
            join_normalized(b"/home/user", b"/etc/config"),
            b"/home/user/etc/config"
        );
    }

    #[test]
    fn join_normalized_collapses_redundant_separators() {
        assert_eq!(join_normalized(b"/home/user", b"a//b"), b"/home/user/a/b");
    }
}
