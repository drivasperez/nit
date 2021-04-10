use std::path::PathBuf;

use crate::database::Database;
use crate::index::Index;
use crate::refs::Refs;
use crate::workspace::Workspace;

pub struct Repository {
    path: PathBuf,
    database: Option<Database>,
    refs: Option<Refs>,
    index: Option<Index>,
    workspace: Option<Workspace>,
}

impl Repository {
    pub fn new<P: Into<PathBuf>>(path: P) -> Self {
        Self {
            path: path.into(),
            database: None,
            refs: None,
            index: None,
            workspace: None,
        }
    }

    /// Get a mutable reference to the repository's database.
    pub fn database(&mut self) -> &mut Database {
        if self.database.is_none() {
            self.database = Some(Database::new(self.path.join("objects")));
        }

        self.database.as_mut().unwrap()
    }

    /// Get a mutable reference to the repository's refs.
    pub fn refs(&mut self) -> &mut Refs {
        if self.refs.is_none() {
            self.refs = Some(Refs::new(&self.path));
        }

        self.refs.as_mut().unwrap()
    }

    /// Get a mutable reference to the repository's index.
    pub fn index(&mut self) -> &mut Index {
        if self.index.is_none() {
            self.index = Some(Index::new(self.path.join("index")));
        }

        self.index.as_mut().unwrap()
    }

    /// Get a mutable reference to the repository's workspace.
    pub fn workspace(&mut self) -> &mut Workspace {
        if self.workspace.is_none() {
            self.workspace = Some(Workspace::new(self.path.parent().unwrap()));
        }

        self.workspace.as_mut().unwrap()
    }
}
