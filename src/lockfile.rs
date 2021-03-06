use crate::utils::add_extension;
use crate::Result;
use std::io;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::{
    fs::{File, OpenOptions},
    path::Path,
};
use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum LockfileError {
    #[error("Missing parent")]
    MissingParent,
    #[error("Permission to lock file was not granted")]
    NoPermission,
    #[error("Lock was stale")]
    StaleLock,
    #[error("Unexpected IO Error")]
    IoError(#[from] std::io::Error),
    #[error(
        "fatal: Unable to create '{0}': File exists.
Another git process seems to be running in this repository, e.g. an editor opened by 'git commit'.
Please make sure all processes are terminated then try again.
If it still fails, a git process may have crashed in this repository earlier: remove the file manually to continue."
    )]
    LockDenied(PathBuf),
}

// TODO: This API could be better. A call to hold_for_update() should return a struct with a write function.
// Dropping the struct would commit and close the file.
#[derive(Debug)]
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

    pub fn hold_for_update(&mut self) -> Result<()> {
        if self.lock.is_none() {
            let f = OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(&self.lock_path)
                .map_err(|e| match e.kind() {
                    io::ErrorKind::NotFound => LockfileError::MissingParent,
                    io::ErrorKind::PermissionDenied => LockfileError::NoPermission,
                    io::ErrorKind::AlreadyExists => {
                        LockfileError::LockDenied(self.lock_path.clone())
                    }

                    _ => LockfileError::IoError(e),
                });

            self.lock = Some(f?);
        }

        Ok(())
    }

    fn lock(&mut self) -> Result<&mut File> {
        self.lock
            .as_mut()
            .ok_or_else(|| LockfileError::StaleLock.into())
    }

    pub fn commit(&mut self) -> Result<()> {
        let lock = self.lock.take().ok_or(LockfileError::StaleLock);
        drop(lock);
        std::fs::rename(&self.lock_path, &self.file_path)?;

        Ok(())
    }

    pub fn rollback(&mut self) -> Result<()> {
        let lock = self.lock.take().ok_or(LockfileError::StaleLock);
        drop(lock);
        std::fs::remove_file(&self.lock_path)?;

        Ok(())
    }
}

impl Read for Lockfile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.lock()?.read(buf)
    }
}

impl Write for Lockfile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.lock()?.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.lock()?.flush()
    }
}
