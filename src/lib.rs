use database::{Object, Oid};
use std::{borrow::Cow, os::unix::prelude::OsStrExt, path::Path};
use std::{ffi::OsString, path::PathBuf};

pub mod database;

pub struct Workspace {
    pathname: PathBuf,
}

impl Workspace {
    pub fn new<P: Into<PathBuf>>(pathname: P) -> Self {
        Self {
            pathname: pathname.into(),
        }
    }

    pub fn list_files(&self) -> std::io::Result<Vec<PathBuf>> {
        let dirs = std::fs::read_dir(&self.pathname)?;
        let mut filtered_dirs = Vec::new();
        for dir in dirs {
            let path = dir?.path();
            if !&[".", "..", ".git"].iter().any(|&s| path.ends_with(s)) {
                filtered_dirs.push(path);
            }
        }

        Ok(filtered_dirs)
    }

    pub fn read_file<P: AsRef<Path>>(&self, path: P) -> std::io::Result<Vec<u8>> {
        std::fs::read(&self.pathname.join(&path))
    }
}

pub struct Blob {
    oid: Option<Oid>,
    data: Vec<u8>,
}

impl Blob {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data, oid: None }
    }

    pub fn to_bytestr(&self) -> &[u8] {
        &self.data
    }

    pub fn set_oid(&mut self, oid: Oid) {
        self.oid = Some(oid);
    }

    pub fn oid(&self) -> Option<&Oid> {
        self.oid.as_ref()
    }
}

impl Object for Blob {
    fn data(&self) -> Cow<[u8]> {
        Cow::Borrowed(self.to_bytestr())
    }

    fn kind(&self) -> &str {
        "blob"
    }

    fn set_oid(&mut self, oid: Oid) {
        self.oid = Some(oid);
    }
}

#[derive(Debug)]
pub struct Entry {
    name: OsString,
    oid: Oid,
}

impl Entry {
    pub fn new(path: &PathBuf, oid: Oid) -> Self {
        let name = path.file_name().unwrap().to_owned();
        Self { name, oid }
    }
}

#[derive(Debug)]
pub struct Tree {
    oid: Option<Oid>,
    entries: Vec<Entry>,
}

impl Tree {
    pub fn new(mut entries: Vec<Entry>) -> Self {
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        Self { entries, oid: None }
    }

    pub fn oid(&self) -> Option<&Oid> {
        self.oid.as_ref()
    }
}

const MODE: &[u8] = b"100644";

impl Object for Tree {
    fn data(&self) -> Cow<[u8]> {
        let data: Vec<u8> = self
            .entries
            .iter()
            .flat_map(|entry| {
                let mut bytes = Vec::new();
                bytes.extend_from_slice(MODE);
                bytes.extend_from_slice(b" ");
                bytes.extend_from_slice(entry.name.as_bytes());
                bytes.extend_from_slice(&['\0' as u8]);
                bytes.extend_from_slice(entry.oid.bytes());
                bytes
            })
            .collect();
        Cow::Owned(data)
    }

    fn kind(&self) -> &str {
        "tree"
    }

    fn set_oid(&mut self, oid: Oid) {
        self.oid = Some(oid);
    }
}
