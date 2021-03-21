use chrono::{DateTime, Utc};
use database::{Object, ObjectId};
use std::{borrow::Cow, fmt::Display, os::unix::prelude::OsStrExt, path::Path};
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
    oid: Option<ObjectId>,
    data: Vec<u8>,
}

impl Blob {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data, oid: None }
    }

    pub fn to_bytestr(&self) -> &[u8] {
        &self.data
    }

    pub fn set_oid(&mut self, oid: ObjectId) {
        self.oid = Some(oid);
    }

    pub fn oid(&self) -> Option<&ObjectId> {
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

    fn set_oid(&mut self, oid: ObjectId) {
        self.oid = Some(oid);
    }
}

#[derive(Debug)]
pub struct Entry {
    name: OsString,
    oid: ObjectId,
}

impl Entry {
    pub fn new(path: &PathBuf, oid: ObjectId) -> Self {
        let name = path.file_name().unwrap().to_owned();
        Self { name, oid }
    }
}

#[derive(Debug)]
pub struct Tree {
    oid: Option<ObjectId>,
    entries: Vec<Entry>,
}

impl Tree {
    pub fn new(mut entries: Vec<Entry>) -> Self {
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        Self { entries, oid: None }
    }

    pub fn oid(&self) -> Option<&ObjectId> {
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

    fn set_oid(&mut self, oid: ObjectId) {
        self.oid = Some(oid);
    }
}

#[derive(Clone, Debug)]
pub struct Author {
    name: String,
    email: String,
    time: DateTime<Utc>,
}

impl Author {
    pub fn new(name: String, email: String, time: DateTime<Utc>) -> Self {
        Self { name, email, time }
    }
}

impl Display for Author {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} <{}> {}",
            self.name,
            self.email,
            self.time.format("%s %z")
        )
    }
}

pub struct Commit {
    author: Author,
    message: String,
    tree: ObjectId,
    oid: Option<ObjectId>,
}

impl Commit {
    pub fn new(tree_oid: ObjectId, author: Author, message: String) -> Self {
        Self {
            author,
            tree: tree_oid,
            message,
            oid: None,
        }
    }

    pub fn oid(&self) -> Option<&ObjectId> {
        self.oid.as_ref()
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl Object for Commit {
    fn data(&self) -> Cow<[u8]> {
        let data = format!(
            "tree {}\nauthor {}\ncommitter {}\n\n{}",
            self.tree, self.author, self.author, self.message
        );

        Cow::Owned(data.into_bytes())
    }

    fn kind(&self) -> &str {
        "commit"
    }

    fn set_oid(&mut self, oid: ObjectId) {
        self.oid = Some(oid);
    }
}
