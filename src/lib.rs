use thiserror::Error;
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
    #[error("Lockfile error")]
    Lockfile(#[from] lockfile::LockfileError),
    #[error("Database error")]
    Database(#[from] database::DatabaseError),
    #[error("Ref error")]
    Ref(#[from] refs::RefError),
}
