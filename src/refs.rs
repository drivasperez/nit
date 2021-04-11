use crate::lockfile::Lockfile;
use crate::{database::ObjectId, lockfile::LockfileError};
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::Result;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RefError {
    #[error("Couldn't get lock")]
    NoLock(#[from] LockfileError),
    #[error("Couildn't write to lockfile")]
    CouldNotWrite(#[from] std::io::Error),
    #[error("Couldn't get lockfile id")]
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

    pub fn update_head(&self, oid: &ObjectId) -> Result<()> {
        let mut lock = Lockfile::new(&self.head_path());
        lock.hold_for_update()?;

        lock.write_all(&oid.as_str()?.as_bytes())?;
        lock.write_all(b"\n")?;

        lock.commit()?;

        Ok(())
    }

    pub fn read_head(&self) -> Option<String> {
        let bytes = std::fs::read(self.head_path()).ok()?;
        let s = String::from_utf8(bytes).ok()?;

        Some(s)
    }
}
