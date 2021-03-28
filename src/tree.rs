use std::ffi::OsString;
use std::{borrow::Cow, fs};
use std::{os::unix::prelude::MetadataExt, path::PathBuf};

use crate::database::{Object, ObjectId};
#[derive(Debug, PartialEq, Copy, Clone)]
pub enum EntryMode {
    Executable,
    Regular,
}

impl From<fs::Metadata> for EntryMode {
    fn from(metadata: fs::Metadata) -> Self {
        let mode = metadata.mode();
        match (mode & 0o111) != 0 {
            true => Self::Executable,
            false => Self::Regular,
        }
    }
}
#[derive(Debug, PartialEq)]
pub struct Entry {
    name: OsString,
    oid: ObjectId,
    mode: EntryMode,
}

impl Entry {
    pub fn new(path: &OsString, oid: ObjectId, mode: EntryMode) -> Self {
        let name = path.to_owned();
        Self { name, oid, mode }
    }

    pub fn parent_directories(&self) -> Vec<PathBuf> {
        let path = PathBuf::from(&self.name);
        let mut directories: Vec<_> = path.ancestors().map(|c| c.to_owned()).skip(1).collect();

        directories.pop();

        directories.into_iter().rev().collect()
    }
}

#[derive(Debug, PartialEq)]
pub enum TreeEntry {
    Tree(PathBuf),
    Object(Entry),
}

#[derive(Debug)]
pub struct Tree {
    arena: Arena<TreeEntry>,
}

impl Tree {
    pub fn new() -> Self {
        Self {
            arena: Arena::new(),
        }
    }

    pub fn traverse<F>(&mut self, func: F)
    where
        F: FnMut(Tree) -> anyhow::Result<ObjectId>,
    {
        todo!()
    }

    pub fn build(mut entries: Vec<Entry>) -> Self {
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        let arena = Arena::new();
        let mut root = Self { arena };

        for entry in entries {
            root.add_entry(entry);
        }

        root
    }

    pub fn add_entry(&mut self, entry: Entry) {
        let parents: Vec<TreeEntry> = entry
            .parent_directories()
            .into_iter()
            .map(TreeEntry::Tree)
            .collect();

        if parents.is_empty() {
            self.arena.insert(None, TreeEntry::Object(entry));
        } else {
            let mut parent_idx = None;
            for parent in parents {
                parent_idx = match self.arena.includes(&parent) {
                    Some(idx) => Some(idx),
                    None => {
                        let next_idx = self.arena.node(parent);
                        self.arena.arena[next_idx].parent = parent_idx;
                        if let Some(i) = parent_idx {
                            self.arena.arena[i].children.push(next_idx);
                        }

                        Some(next_idx)
                    }
                }
            }

            let parent_idx = parent_idx.unwrap();
            let idx = self.arena.node(TreeEntry::Object(entry));
            self.arena.arena[idx].parent = Some(parent_idx);
            self.arena.arena[parent_idx].children.push(idx);
        }
    }
}

const REGULAR_MODE: &[u8] = b"100644";
const EXECUTABLE_MODE: &[u8] = b"100755";

impl Object for Tree {
    fn data(&self) -> Cow<[u8]> {
        // let data: Vec<u8> = self
        //     .arena
        //     .iter()
        //     .flat_map(|entry| {
        //         let mut bytes = Vec::new();
        //         bytes.extend_from_slice(match entry.mode {
        //             EntryMode::Executable => EXECUTABLE_MODE,
        //             EntryMode::Regular => REGULAR_MODE,
        //         });
        //         bytes.extend_from_slice(b" ");
        //         bytes.extend_from_slice(entry.name.as_bytes());
        //         bytes.push(b'\0');
        //         bytes.extend_from_slice(entry.oid.bytes());
        //         bytes
        //     })
        //     .collect();
        // Cow::Owned(data)

        todo!()
    }

    fn kind(&self) -> &str {
        "tree"
    }
}

#[derive(Debug, PartialEq)]
pub struct Arena<T: PartialEq> {
    arena: Vec<Node<T>>,
}

impl<T: PartialEq> Arena<T> {
    pub fn new() -> Self {
        Self { arena: vec![] }
    }

    pub fn node(&mut self, val: T) -> usize {
        for node in &self.arena {
            if node.val == val {
                return node.idx;
            }
        }

        let idx = self.arena.len();
        self.arena.push(Node::new(idx, val));
        idx
    }

    pub fn includes(&self, val: &T) -> Option<usize> {
        for node in &self.arena {
            if node.val == *val {
                return Some(node.idx);
            }
        }

        None
    }

    pub fn insert(&mut self, parent: Option<T>, node: T) {
        let node_idx = self.node(node);
        if let Some(parent) = parent {
            let parent_idx = self.node(parent);
            self.arena[node_idx].parent = Some(node_idx);
            self.arena[parent_idx].children.push(node_idx);
        }
    }
}

#[derive(Debug, PartialEq)]
struct Node<T: PartialEq> {
    idx: usize,
    val: T,
    parent: Option<usize>,
    children: Vec<usize>,
}

impl<T: PartialEq> Node<T> {
    fn new(idx: usize, val: T) -> Self {
        Self {
            idx,
            val,
            parent: None,
            children: vec![],
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parent_directories() {
        let entry = Entry {
            name: r"bin/nested/jit".into(),
            oid: ObjectId::new([0; 20]),
            mode: EntryMode::Executable,
        };

        let parents = entry.parent_directories();

        assert_eq!(
            parents,
            vec![PathBuf::from("bin"), PathBuf::from("bin/nested")]
        )
    }

    #[test]
    fn arena_hmm() {
        let entry1 = Entry {
            name: "blah.rb".into(),
            oid: ObjectId::new([1; 20]),
            mode: EntryMode::Executable,
        };
        let entry2 = Entry {
            name: "cool/beans/blah.rb".into(),
            oid: ObjectId::new([1; 20]),
            mode: EntryMode::Executable,
        };

        let entries = vec![entry1, entry2];

        let tree = Tree::build(entries);

        assert_eq!(tree.arena, Arena::new());
    }
}
