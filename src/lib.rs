use thiserror::Error;
pub mod database;
pub mod index;
pub mod lockfile;
pub mod refs;
pub mod repository;
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
