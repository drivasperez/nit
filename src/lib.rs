use std::{
    collections::BTreeSet,
    fs::Metadata,
    path::{Path, PathBuf},
};

use index::Index;
use thiserror::Error;
use workspace::Workspace;
pub mod database;
pub mod index;
pub mod lockfile;
pub mod refs;
pub mod workspace;

mod utils;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Workspace error")]
    Workspace(#[from] workspace::WorkspaceError),
    #[error("Index error")]
    Index(#[from] index::IndexError),
    #[error("Checksum error")]
    Checksum(#[from] index::checksum::ChecksumError),
    #[error("Lockfile error")]
    Lockfile(#[from] lockfile::LockfileError),
    #[error("Database error")]
    Database(#[from] database::DatabaseError),
    #[error("Ref error")]
    Ref(#[from] refs::RefError),
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error(transparent)]
    FmtError(#[from] std::fmt::Error),
}

pub type Result<T, E = Error> = core::result::Result<T, E>;

impl From<crate::Error> for std::io::Error {
    fn from(err: crate::Error) -> Self {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Could get lock for file: {}", err),
        )
    }
}

pub struct Status {
    root_path: PathBuf,
    untracked: BTreeSet<String>,
    workspace: Workspace,
    index: Index,
}

impl Status {
    pub fn new(path: &impl AsRef<Path>) -> Self {
        let root_path = path.as_ref().to_path_buf();
        let workspace = Workspace::new(&root_path);
        let index = Index::new(&root_path.join(".git").join("index"));
        Self {
            untracked: BTreeSet::new(),
            root_path,
            workspace,
            index,
        }
    }

    pub fn get(&mut self) -> Result<String> {
        self.untracked.clear();

        self.scan_workspace(None)?;

        let status = self
            .untracked
            .iter()
            .map(|s| format!("?? {}", s))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(status)
    }

    fn scan_workspace(&mut self, path: Option<&Path>) -> Result<()> {
        self.index.load_for_update()?;

        let path = path.unwrap_or(&self.root_path);

        for (dir, metadata) in self.workspace.list_dir(&path)? {
            if self.index.is_tracked(&dir) {
                if metadata.is_dir() {
                    self.scan_workspace(Some(&dir))?;
                }
            } else if self.is_trackable_file(&dir, &metadata)? {
                let mut dir = dir.to_string_lossy().into_owned();
                if metadata.is_dir() {
                    dir.push(std::path::MAIN_SEPARATOR);
                }

                self.untracked.insert(dir);
            }
        }

        Ok(())
    }

    fn is_trackable_file(&self, path: &Path, stat: &Metadata) -> Result<bool> {
        if stat.is_file() {
            return Ok(!self.index.is_tracked(&path));
        }

        if !stat.is_dir() {
            return Ok(false);
        }

        let items = self.workspace.list_dir(&path)?;
        let (files, directories): (Vec<_>, Vec<_>) =
            items.iter().partition(|(_, metadata)| metadata.is_file());

        for (path, stat) in files.iter().chain(directories.iter()) {
            if self.is_trackable_file(path, stat)? {
                return Ok(true);
            }
        }

        Ok(false)
    }
}
