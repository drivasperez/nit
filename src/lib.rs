use chrono::{DateTime, Utc};
use database::{Object, ObjectId};
use std::{borrow::Cow, fmt::Display};

pub mod database;
pub mod lockfile;
pub mod refs;
pub mod tree;
pub mod workspace;

mod arena;
mod utils;

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
