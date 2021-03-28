use std::ffi::OsString;
use std::{borrow::Cow, fs};
use std::{os::unix::prelude::MetadataExt, path::PathBuf};

use crate::{
    arena::{Arena, Key},
    database::{Object, ObjectId},
};
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

    pub fn traverse<F>(&mut self, _func: F)
    where
        F: FnMut(Tree) -> anyhow::Result<ObjectId>,
    {
        todo!()
    }

    pub fn build(mut entries: Vec<Entry>) -> anyhow::Result<Self> {
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        let arena = Arena::new();
        let mut root = Self { arena };

        for entry in entries {
            root.add_entry(entry)?;
        }

        Ok(root)
    }

    pub fn add_entry(&mut self, entry: Entry) -> anyhow::Result<()> {
        let parents: Vec<TreeEntry> = entry
            .parent_directories()
            .into_iter()
            .map(TreeEntry::Tree)
            .collect();

        if parents.is_empty() {
            self.arena.insert(TreeEntry::Object(entry));
        } else {
            let mut parent: Option<Key> = None;
            for p in parents {
                parent = match self.arena.get_token(&p) {
                    Some(node) => Some(node),
                    None => {
                        let child = self.arena.insert(p);
                        if let Some(parent) = parent {
                            self.arena.append(child, parent)?;
                        }

                        Some(child)
                    }
                }
            }

            let parent = parent.unwrap();
            let node = self.arena.insert(TreeEntry::Object(entry));
            self.arena.append(node, parent)?;
        }

        Ok(())
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
}
