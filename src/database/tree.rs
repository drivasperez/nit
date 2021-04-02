use std::{borrow::Cow, collections::BTreeMap, fs};
use std::{ffi::OsString, os::unix::prelude::OsStrExt};
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
    Tree(Tree, Option<ObjectId>),
    Object(Entry),
}

#[derive(Debug, PartialEq)]
pub struct Tree {
    entries: BTreeMap<OsString, TreeEntry>,
}

impl Tree {
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    pub fn traverse<F>(&mut self, func: &F) -> anyhow::Result<ObjectId>
    where
        F: Fn(&Tree) -> anyhow::Result<ObjectId>,
    {
        for (_name, entry) in &mut self.entries {
            if let TreeEntry::Tree(tree, oid) = entry {
                let tree_oid = tree.traverse(func)?;
                *oid = Some(tree_oid);
            }
        }

        func(self)
    }

    pub fn build(mut entries: Vec<Entry>) -> Self {
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        let mut root = Tree::new();

        for entry in entries {
            let parents = entry.parent_directories();
            root.add_entry(parents, entry);
        }

        dbg!(root)
    }

    pub fn add_entry(&mut self, parents: Vec<PathBuf>, entry: Entry) {
        if parents.is_empty() {
            self.entries
                .insert(entry.name.clone(), TreeEntry::Object(entry));
        } else {
            let tree = self
                .entries
                .entry(parents[0].file_name().unwrap().to_owned())
                .or_insert(TreeEntry::Tree(Tree::new(), None));

            if let TreeEntry::Tree(tree, _) = tree {
                tree.add_entry(
                    parents.iter().skip(1).map(|c| c.to_owned()).collect(),
                    entry,
                )
            }
        }
    }
}

const REGULAR_MODE: &[u8] = b"100644";
const EXECUTABLE_MODE: &[u8] = b"100755";
const DIRECTORY_MODE: &[u8] = b"40000";

impl Object for Tree {
    fn data(&self) -> Cow<[u8]> {
        let data: Vec<u8> = self
            .entries
            .iter()
            .flat_map(|(name, entry)| match &entry {
                &TreeEntry::Object(entry) => {
                    let mut bytes = Vec::new();
                    bytes.extend_from_slice(match entry.mode {
                        EntryMode::Executable => EXECUTABLE_MODE,
                        EntryMode::Regular => REGULAR_MODE,
                    });
                    bytes.extend_from_slice(b" ");
                    bytes.extend_from_slice(name.as_bytes());
                    bytes.push(b'\0');
                    bytes.extend_from_slice(entry.oid.bytes());
                    bytes
                }
                &TreeEntry::Tree(_, oid) => {
                    let mut bytes = Vec::new();
                    bytes.extend_from_slice(DIRECTORY_MODE);
                    bytes.extend_from_slice(b" ");
                    bytes.extend_from_slice(name.as_bytes());
                    bytes.push(b'\0');
                    bytes.extend_from_slice(
                        oid.as_ref()
                            .expect("Fatal: Couldn't unwrap Tree's ObjectID")
                            .bytes(),
                    );
                    bytes
                }
            })
            .collect();
        Cow::Owned(data)
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
            oid: ObjectId([0; 20]),
            mode: EntryMode::Executable,
        };

        let parents = entry.parent_directories();

        assert_eq!(
            parents,
            vec![PathBuf::from("bin"), PathBuf::from("bin/nested")]
        )
    }
}
