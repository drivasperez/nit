use std::{borrow::Cow, fs};
use std::{ffi::OsString, os::unix::prelude::OsStrExt};
use std::{os::unix::prelude::MetadataExt, path::PathBuf};

use crate::database::{Object, ObjectId};
#[derive(Debug, Copy, Clone)]
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
#[derive(Debug)]
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

#[derive(Debug)]
pub struct Tree {
    entries: Vec<Entry>,
}

impl Tree {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn build(mut entries: Vec<Entry>) -> Self {
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        let mut root = Tree::new();
        for entry in entries {
            root.add_entry(entry);
        }

        root
    }

    pub fn add_entry(&mut self, entry: Entry) {
        let parents = entry.parent_directories();
    }
}

const REGULAR_MODE: &[u8] = b"100644";
const EXECUTABLE_MODE: &[u8] = b"100755";

impl Object for Tree {
    fn data(&self) -> Cow<[u8]> {
        let data: Vec<u8> = self
            .entries
            .iter()
            .flat_map(|entry| {
                let mut bytes = Vec::new();
                bytes.extend_from_slice(match entry.mode {
                    EntryMode::Executable => EXECUTABLE_MODE,
                    EntryMode::Regular => REGULAR_MODE,
                });
                bytes.extend_from_slice(b" ");
                bytes.extend_from_slice(entry.name.as_bytes());
                bytes.push(b'\0');
                bytes.extend_from_slice(entry.oid.bytes());
                bytes
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
