use crate::lockfile::Lockfile;
use crate::{database::ObjectId, lockfile::LockfileError};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RefError {
    #[error("Couldn't get lock: {0}")]
    NoLock(#[from] LockfileError),
    #[error("Couldn't get lockfile id: {0}")]
    BadObjectId(#[from] std::fmt::Error),
}

pub struct Refs {
    pathname: PathBuf,
}

impl Refs {
    pub fn new(pathname: &Path) -> Self {
        Self {
            pathname: pathname.to_owned(),
        }
    }
    pub fn head_path(&self) -> PathBuf {
        self.pathname.join("HEAD")
    }

    pub fn update_head(&self, oid: &ObjectId) -> Result<(), RefError> {
        let mut lock = Lockfile::new(&self.head_path());
        lock.hold_for_update()?;

        lock.write(&oid.as_str()?.as_bytes())?;
        lock.write(b"\n")?;

        lock.commit()?;

        Ok(())
    }

    pub fn read_head(&self) -> Option<String> {
        let bytes = std::fs::read(self.head_path()).ok()?;
        let s = String::from_utf8(bytes).ok()?;

        Some(s)
    }
}
