use std::borrow::Cow;

use super::{Author, Object, ObjectId};

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
