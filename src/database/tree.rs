use std::{borrow::Cow, collections::BTreeMap, fs};
use std::{ffi::OsString, os::unix::prelude::OsStrExt};
use std::{os::unix::prelude::MetadataExt, path::PathBuf};

use crate::database::{Object, ObjectId};
use crate::index::entry::Entry;

use crate::Result;

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
pub enum TreeEntry {
    Tree(Tree, Option<ObjectId>),
    Object(Entry),
}

#[derive(Debug, Default, PartialEq)]
pub struct Tree {
    entries: BTreeMap<OsString, TreeEntry>,
}

impl Tree {
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    pub fn traverse<F>(&mut self, func: &mut F) -> Result<ObjectId>
    where
        F: FnMut(&Tree) -> Result<ObjectId>,
    {
        for entry in self.entries.values_mut() {
            if let TreeEntry::Tree(tree, oid) = entry {
                let tree_oid = tree.traverse(func)?;
                *oid = Some(tree_oid);
            }
        }

        func(self)
    }

    pub fn build(mut entries: Vec<Entry>) -> Self {
        entries.sort_by(|a, b| a.path().cmp(&b.path()));
        let mut root = Tree::new();

        for entry in entries {
            let parents = entry.parent_directories();
            root.add_entry(parents, entry);
        }

        root
    }

    pub fn add_entry(&mut self, parents: Vec<PathBuf>, entry: Entry) {
        if parents.is_empty() {
            self.entries.insert(
                entry.path().as_os_str().to_owned(),
                TreeEntry::Object(entry),
            );
        } else {
            let tree = self
                .entries
                .entry(parents[0].file_name().unwrap().to_owned())
                .or_insert_with(|| TreeEntry::Tree(Tree::new(), None));

            if let TreeEntry::Tree(tree, _) = tree {
                tree.add_entry(
                    parents.iter().skip(1).map(|c| c.to_owned()).collect(),
                    entry,
                )
            }
        }
    }
}

const DIRECTORY_MODE: u32 = 0o40000;

impl Object for Tree {
    fn data(&self) -> Cow<[u8]> {
        let data: Vec<u8> = self
            .entries
            .iter()
            .flat_map(|(name, entry)| match &entry {
                TreeEntry::Object(entry) => {
                    let mut bytes = Vec::new();
                    bytes.extend_from_slice(format!("{:o}", entry.mode()).as_bytes());
                    bytes.extend_from_slice(b" ");
                    bytes.extend_from_slice(name.as_bytes());
                    bytes.push(b'\0');
                    bytes.extend_from_slice(entry.oid().bytes());
                    bytes
                }
                TreeEntry::Tree(_, oid) => {
                    let mut bytes = Vec::new();
                    bytes.extend_from_slice(format!("{:o}", DIRECTORY_MODE).as_bytes());
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
        {
            let metadata = fs::metadata("./Cargo.toml").unwrap();
            let entry = Entry::new(&r"bin/nested/jit", ObjectId([0; 20]), metadata);

            let parents = entry.parent_directories();

            assert_eq!(
                parents,
                vec![PathBuf::from("bin"), PathBuf::from("bin/nested")]
            );
        }
    }
}
