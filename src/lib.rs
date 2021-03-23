use chrono::{DateTime, Utc};
use database::{Object, ObjectId};
use std::{
    borrow::Cow,
    fmt::Display,
    fs, io,
    os::unix::prelude::{MetadataExt, OsStrExt},
    path::Path,
};
use std::{ffi::OsString, path::PathBuf};

pub mod database;
pub mod lockfile;
pub mod refs;

mod utils;

pub struct Workspace {
    pathname: PathBuf,
}

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

    pub fn read_file<P: AsRef<Path>>(&self, path: P) -> io::Result<Vec<u8>> {
        std::fs::read(&self.pathname.join(&path))
    }

    pub fn stat_file<P: AsRef<Path>>(&self, path: P) -> io::Result<EntryMode> {
        let metadata = fs::metadata(&path)?;
        Ok(EntryMode::from(metadata))
    }
}

pub struct Blob {
    data: Vec<u8>,
}

impl Blob {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    pub fn to_bytestr(&self) -> &[u8] {
        &self.data
    }
}

impl Object for Blob {
    fn data(&self) -> Cow<[u8]> {
        Cow::Borrowed(self.to_bytestr())
    }

    fn kind(&self) -> &str {
        "blob"
    }
}

#[derive(Debug)]
pub struct Entry {
    name: OsString,
    oid: ObjectId,
    mode: EntryMode,
}

impl Entry {
    pub fn new(path: &PathBuf, oid: ObjectId, mode: EntryMode) -> Self {
        let name = path.file_name().unwrap().to_owned();
        Self { name, oid, mode }
    }
}

#[derive(Debug)]
pub struct Tree {
    entries: Vec<Entry>,
}

impl Tree {
    pub fn new(mut entries: Vec<Entry>) -> Self {
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        Self { entries }
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
    parent: Option<String>,
}

impl Commit {
    pub fn new(parent: Option<&str>, tree_oid: ObjectId, author: Author, message: String) -> Self {
        Self {
            parent: parent.map(|s| s.to_owned()),
            author,
            tree: tree_oid,
            message,
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl Object for Commit {
    fn data(&self) -> Cow<[u8]> {
        let mut data = Vec::new();
        data.push(format!("tree {}", self.tree));
        if let Some(p) = &self.parent {
            data.push(format!("parent {}", p));
        }
        data.push(format!("author {}", self.author));
        data.push(format!("committer {}", self.author));
        data.push(String::from("\n"));
        data.push(self.message.to_owned());

        Cow::Owned(data.join("\n").into_bytes())
    }

    fn kind(&self) -> &str {
        "commit"
    }
}
