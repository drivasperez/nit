use crate::utils::add_extension;
use anyhow::Context;
use std::io::{ErrorKind, Write};
use std::path::PathBuf;
use std::{
    fs::{File, OpenOptions},
    path::Path,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LockfileError {
    #[error("Missing parent")]
    MissingParent,
    #[error("Permission to lock file was not granted")]
    NoPermission,
    #[error("Lock was stale")]
    StaleLock,
    #[error("Unexpected IO Error")]
    UnexpectedError(std::io::ErrorKind),
}

// TODO: This API could be better. A call to hold_for_update() should return a struct with a write function.
// Dropping the struct would commit and close the file.
pub struct Lockfile {
    file_path: PathBuf,
    lock_path: PathBuf,

    lock: Option<File>,
}

impl Lockfile {
    pub fn new(path: &Path) -> Self {
        let file_path = path.to_owned();
        let mut lock_path = path.to_owned();
        add_extension(&mut lock_path, "lock");

        Self {
            lock: None,
            file_path,
            lock_path,
        }

    }

    pub fn hold_for_update(&mut self) -> anyhow::Result<bool> {
        if self.lock.is_none() {
            let f = OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(&self.lock_path)
                .map_err(|e| match e.kind() {
                    ErrorKind::NotFound => (LockfileError::MissingParent),
                    ErrorKind::PermissionDenied => (LockfileError::NoPermission),
                    e => LockfileError::UnexpectedError(e),
                });

            if let Err(LockfileError::UnexpectedError(kind)) = f {
                if kind == ErrorKind::AlreadyExists {
                    return Ok(false);
                }
            } else {
                self.lock = Some(f.context("Couldn't take lock")?);
            }
        }

        Ok(true)
    }

    pub fn write(&mut self, contents: &str) -> anyhow::Result<()> {
        let lock = self.lock.as_mut().ok_or(LockfileError::StaleLock)?;

        lock.write(contents.as_bytes())
            .map_err(|e| LockfileError::UnexpectedError(e.kind()))
            .context("Couldn't write to lock file")?;

        Ok(())
    }

    pub fn commit(&mut self) -> anyhow::Result<()> {
        let lock = self
            .lock
            .take()
            .ok_or(LockfileError::StaleLock)
            .context("Couldn't drop lock")?;
        drop(lock);
        std::fs::rename(&self.lock_path, &self.file_path)
            .map_err(|e| LockfileError::UnexpectedError(e.kind()))
            .context("Couldn't rename lock file")?;

        Ok(())
    }
}
