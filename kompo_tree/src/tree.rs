//! Flat arena over the embedded path table.
//!
//! The generated C data gives us two index-aligned blobs: `PATHS` (NUL
//! separated paths) and `FILES` (concatenated bodies, sliced by `FILES_SIZES`).
//! This module turns the first into a tree without giving up that alignment:
//! node ids are handed out in `PATHS` order, so a node id, its path bytes and
//! its file body all sit at corresponding positions in their arrays.
//!
//! Path resolution goes through one hash of the whole path. That costs a single
//! independent random access instead of one dependent load per component, which
//! measures several times faster than walking the arena even though the walk
//! has better locality. The walk is kept for the spellings the hash cannot
//! answer (`..`, `//`, trailing slash).

use rustc_hash::FxHashMap;
use std::cmp::Ordering;

/// Index into the node arena. Node 0 is always the root directory.
pub type NodeId = u32;

/// The root directory.
pub const ROOT: NodeId = 0;

/// `file_idx` of a directory: it has no body in `FILES`.
const NO_FILE: u32 = u32::MAX;
/// `name_slot` of the root: it is nobody's child.
const NO_SLOT: u32 = u32::MAX;

#[derive(Clone, Copy, Debug)]
struct Node {
    parent: NodeId,
    /// Slot in `children` where this node's own name lives.
    name_slot: u32,
    /// Range in `children` holding this node's entries.
    child_start: u32,
    child_len: u32,
    /// Index into `file_offsets`, or `NO_FILE` for a directory.
    file_idx: u32,
}

pub struct Tree<'a> {
    nodes: Box<[Node]>,
    /// Child ids grouped by parent, each group sorted by name.
    children: Box<[NodeId]>,
    /// Offsets into `name_blob`, aligned with `children` (len = children + 1).
    name_off: Box<[u32]>,
    /// Child names in slot order, so one directory's names are contiguous.
    name_blob: Box<[u8]>,
    /// Canonical absolute path -> node. Keys borrow the caller's path blob.
    by_path: FxHashMap<&'a [u8], NodeId>,
    files: &'a [u8],
    file_offsets: &'a [u64],
}

impl<'a> Tree<'a> {
    /// Build from the embedded blobs.
    ///
    /// `paths` is NUL separated, `file_offsets` has one more entry than there
    /// are paths, and body `i` is `files[file_offsets[i]..file_offsets[i + 1]]`.
    pub fn build(paths: &'a [u8], files: &'a [u8], file_offsets: &'a [u64]) -> Self {
        let mut nodes = vec![Node {
            parent: ROOT,
            name_slot: NO_SLOT,
            child_start: 0,
            child_len: 0,
            file_idx: NO_FILE,
        }];
        let mut kids: Vec<Vec<NodeId>> = vec![Vec::new()];
        let mut names: Vec<&'a [u8]> = vec![&[]];
        let mut by_component: FxHashMap<(NodeId, &'a [u8]), NodeId> = FxHashMap::default();
        let mut by_path: FxHashMap<&'a [u8], NodeId> = FxHashMap::default();
        by_path.insert(b"/", ROOT);

        // Entries are visited in blob order, so node ids follow PATHS order.
        for (file_idx, entry) in split_paths(paths).enumerate() {
            if entry.is_empty() {
                continue; // keeps file_idx aligned with file_offsets
            }

            let mut cur = ROOT;
            let mut i = 0usize;
            while i < entry.len() {
                while i < entry.len() && entry[i] == b'/' {
                    i += 1;
                }
                if i >= entry.len() {
                    break;
                }
                let start = i;
                while i < entry.len() && entry[i] != b'/' {
                    i += 1;
                }
                let comp = &entry[start..i];

                cur = match by_component.get(&(cur, comp)) {
                    Some(&id) => id,
                    None => {
                        let id = nodes.len() as NodeId;
                        nodes.push(Node {
                            parent: cur,
                            name_slot: NO_SLOT,
                            child_start: 0,
                            child_len: 0,
                            file_idx: NO_FILE,
                        });
                        kids.push(Vec::new());
                        names.push(comp);
                        kids[cur as usize].push(id);
                        by_component.insert((cur, comp), id);
                        id
                    }
                };

                // `entry[..i]` is the absolute path of the node we just reached.
                by_path.entry(&entry[..i]).or_insert(cur);
            }

            // The component the loop ended on is the file itself.
            if cur != ROOT {
                nodes[cur as usize].file_idx = file_idx as u32;
            }
        }

        // Flatten the per-parent lists into one array. Parents are still in
        // PATHS order, so sibling groups stay grouped by gem as emitted.
        let mut children: Vec<NodeId> = Vec::with_capacity(nodes.len());
        let mut name_blob: Vec<u8> = Vec::new();
        let mut name_off: Vec<u32> = vec![0];
        for parent in 0..nodes.len() {
            let mut group = std::mem::take(&mut kids[parent]);
            group.sort_unstable_by(|&x, &y| names[x as usize].cmp(names[y as usize]));

            nodes[parent].child_start = children.len() as u32;
            nodes[parent].child_len = group.len() as u32;
            for id in group {
                nodes[id as usize].name_slot = children.len() as u32;
                children.push(id);
                name_blob.extend_from_slice(names[id as usize]);
                name_off.push(name_blob.len() as u32);
            }
        }

        Tree {
            nodes: nodes.into_boxed_slice(),
            children: children.into_boxed_slice(),
            name_off: name_off.into_boxed_slice(),
            name_blob: name_blob.into_boxed_slice(),
            by_path,
            files,
            file_offsets,
        }
    }

    /// Resolve an absolute path.
    ///
    /// Canonical spellings are answered by a single hash probe. Anything else
    /// falls back to walking the arena, which also handles `.` and `..`.
    #[inline]
    pub fn lookup(&self, path: &[u8]) -> Option<NodeId> {
        if let Some(&id) = self.by_path.get(path) {
            return Some(id);
        }
        // Every canonical path is in the index, so a miss here is a real ENOENT
        // and must not pay for a second traversal -- that is the hot case
        // during `require`, which probes far more names than it finds.
        if is_canonical(path) {
            return None;
        }
        self.lookup_walk(path)
    }

    /// Resolve by walking the arena one component at a time.
    pub fn lookup_walk(&self, path: &[u8]) -> Option<NodeId> {
        let mut cur = ROOT;
        for comp in path.split(|&b| b == b'/') {
            match comp {
                b"" | b"." => continue,
                b".." => cur = self.nodes.get(cur as usize)?.parent,
                _ => cur = self.child(cur, comp)?,
            }
        }
        Some(cur)
    }

    /// Look up a single entry in a directory.
    #[inline]
    pub fn child(&self, dir: NodeId, name: &[u8]) -> Option<NodeId> {
        let node = self.nodes.get(dir as usize)?;
        let base = node.child_start as usize;
        let (mut lo, mut hi) = (0usize, node.child_len as usize);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            match self.slot_name(base + mid)?.cmp(name) {
                Ordering::Less => lo = mid + 1,
                Ordering::Greater => hi = mid,
                Ordering::Equal => return self.children.get(base + mid).copied(),
            }
        }
        None
    }

    #[inline]
    fn slot_name(&self, slot: usize) -> Option<&[u8]> {
        let start = *self.name_off.get(slot)? as usize;
        let end = *self.name_off.get(slot + 1)? as usize;
        self.name_blob.get(start..end)
    }

    #[inline]
    pub fn is_dir(&self, id: NodeId) -> bool {
        self.nodes
            .get(id as usize)
            .is_some_and(|n| n.file_idx == NO_FILE)
    }

    #[inline]
    pub fn exists(&self, id: NodeId) -> bool {
        (id as usize) < self.nodes.len()
    }

    /// File body, or `None` for a directory.
    #[inline]
    pub fn data(&self, id: NodeId) -> Option<&'a [u8]> {
        let idx = self.nodes.get(id as usize)?.file_idx;
        if idx == NO_FILE {
            return None;
        }
        let start = *self.file_offsets.get(idx as usize)? as usize;
        let end = *self.file_offsets.get(idx as usize + 1)? as usize;
        self.files.get(start..end)
    }

    /// Basename. The root reports `/`.
    #[inline]
    pub fn name(&self, id: NodeId) -> &[u8] {
        match self.nodes.get(id as usize) {
            Some(n) if n.name_slot != NO_SLOT => {
                self.slot_name(n.name_slot as usize).unwrap_or(&[])
            }
            Some(_) => b"/",
            None => &[],
        }
    }

    #[inline]
    pub fn parent(&self, id: NodeId) -> NodeId {
        self.nodes.get(id as usize).map_or(ROOT, |n| n.parent)
    }

    #[inline]
    pub fn child_count(&self, dir: NodeId) -> u32 {
        self.nodes.get(dir as usize).map_or(0, |n| n.child_len)
    }

    /// `index`-th entry of `dir`, in name order.
    #[inline]
    pub fn child_at(&self, dir: NodeId, index: u32) -> Option<NodeId> {
        let node = self.nodes.get(dir as usize)?;
        if index >= node.child_len {
            return None;
        }
        self.children
            .get(node.child_start as usize + index as usize)
            .copied()
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Bytes held by the arena itself, excluding the borrowed blobs.
    pub fn heap_bytes(&self) -> usize {
        self.nodes.len() * std::mem::size_of::<Node>()
            + self.children.len() * 4
            + self.name_off.len() * 4
            + self.name_blob.len()
            + self.by_path.len() * (std::mem::size_of::<(&[u8], NodeId)>() + 1)
    }
}

/// Split the NUL separated path blob, dropping the terminator.
fn split_paths(blob: &[u8]) -> impl Iterator<Item = &[u8]> {
    blob.split_inclusive(|&b| b == 0).map(|entry| {
        if entry.last() == Some(&0) {
            &entry[..entry.len() - 1]
        } else {
            entry
        }
    })
}

/// Is this path spelled exactly the way the index stores it?
///
/// Only canonical paths are guaranteed present, so this is what lets a hash
/// miss be reported as ENOENT without a fallback traversal.
fn is_canonical(path: &[u8]) -> bool {
    if path.first() != Some(&b'/') {
        return false;
    }
    if path == b"/" {
        return true;
    }
    if path.last() == Some(&b'/') {
        return false;
    }
    path[1..]
        .split(|&b| b == b'/')
        .all(|comp| !comp.is_empty() && comp != b"." && comp != b"..")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> Tree<'static> {
        // "/usr/bin/ls", "/usr/bin/cat", "/usr/bin/hoge/fuga", "/usr/empty"
        let paths: &'static [u8] = b"/usr/bin/ls\0/usr/bin/cat\0/usr/bin/hoge/fuga\0/usr/empty\0";
        let files: &'static [u8] = b"LSCATFUGA";
        let offsets: &'static [u64] = &[0, 2, 5, 9, 9];
        Tree::build(paths, files, offsets)
    }

    #[test]
    fn resolves_files_and_directories() {
        let t = tree();
        let ls = t.lookup(b"/usr/bin/ls").unwrap();
        assert_eq!(t.data(ls), Some(&b"LS"[..]));
        assert!(!t.is_dir(ls));

        let bin = t.lookup(b"/usr/bin").unwrap();
        assert!(t.is_dir(bin));
        assert_eq!(t.data(bin), None);

        assert!(t.is_dir(t.lookup(b"/").unwrap()));
        assert!(t.is_dir(t.lookup(b"/usr/bin/hoge").unwrap()));
    }

    #[test]
    fn empty_file_is_a_file_not_a_directory() {
        let t = tree();
        let empty = t.lookup(b"/usr/empty").unwrap();
        assert!(!t.is_dir(empty));
        assert_eq!(t.data(empty), Some(&b""[..]));
    }

    #[test]
    fn misses_are_reported() {
        let t = tree();
        assert_eq!(t.lookup(b"/usr/bin/nope"), None);
        assert_eq!(t.lookup(b"/nope"), None);
        assert_eq!(t.lookup(b"/usr/bin/ls/deeper"), None);
    }

    #[test]
    fn non_canonical_spellings_still_resolve() {
        let t = tree();
        let ls = t.lookup(b"/usr/bin/ls").unwrap();
        assert_eq!(t.lookup(b"/usr//bin/ls"), Some(ls));
        assert_eq!(t.lookup(b"/usr/bin/./ls"), Some(ls));
        assert_eq!(t.lookup(b"/usr/bin/hoge/../ls"), Some(ls));
        assert_eq!(t.lookup(b"/usr/bin/"), t.lookup(b"/usr/bin"));
        assert_eq!(t.lookup(b"/usr/bin/../../usr/bin/ls"), Some(ls));
    }

    #[test]
    fn entries_are_sorted_and_complete() {
        let t = tree();
        let bin = t.lookup(b"/usr/bin").unwrap();
        let names: Vec<&[u8]> = (0..t.child_count(bin))
            .map(|i| t.name(t.child_at(bin, i).unwrap()))
            .collect();
        assert_eq!(names, vec![&b"cat"[..], b"hoge", b"ls"]);
        assert_eq!(t.child_at(bin, 3), None);
    }

    #[test]
    fn parent_links_reach_the_root() {
        let t = tree();
        let fuga = t.lookup(b"/usr/bin/hoge/fuga").unwrap();
        let hoge = t.parent(fuga);
        assert_eq!(t.name(hoge), b"hoge");
        assert_eq!(t.name(t.parent(hoge)), b"bin");
        assert_eq!(t.parent(ROOT), ROOT);
        assert_eq!(t.name(ROOT), b"/");
    }

    #[test]
    fn node_ids_follow_paths_order() {
        // The first path's leaf must get a lower id than the last path's leaf:
        // this is the property that keeps the arena aligned with FILES.
        let t = tree();
        let first = t.lookup(b"/usr/bin/ls").unwrap();
        let last = t.lookup(b"/usr/empty").unwrap();
        assert!(first < last);
    }

    #[test]
    fn canonical_form_detection() {
        assert!(is_canonical(b"/"));
        assert!(is_canonical(b"/a/b.rb"));
        assert!(!is_canonical(b"a/b.rb"));
        assert!(!is_canonical(b"/a//b.rb"));
        assert!(!is_canonical(b"/a/b/"));
        assert!(!is_canonical(b"/a/./b"));
        assert!(!is_canonical(b"/a/../b"));
    }

    #[test]
    fn tolerates_a_blob_without_a_trailing_nul() {
        let paths: &'static [u8] = b"/a.rb\0/b.rb";
        let files: &'static [u8] = b"AB";
        let offsets: &'static [u64] = &[0, 1, 2];
        let t = Tree::build(paths, files, offsets);
        assert_eq!(t.data(t.lookup(b"/a.rb").unwrap()), Some(&b"A"[..]));
        assert_eq!(t.data(t.lookup(b"/b.rb").unwrap()), Some(&b"B"[..]));
    }

    #[test]
    fn truncated_offsets_do_not_panic() {
        let paths: &'static [u8] = b"/a.rb\0/b.rb\0";
        let files: &'static [u8] = b"AB";
        let offsets: &'static [u64] = &[0, 1]; // one short
        let t = Tree::build(paths, files, offsets);
        assert_eq!(t.data(t.lookup(b"/a.rb").unwrap()), Some(&b"A"[..]));
        assert_eq!(t.data(t.lookup(b"/b.rb").unwrap()), None);
    }
}
