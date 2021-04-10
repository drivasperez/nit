use std::io::{Read, Write};

use sha1::{Digest, Sha1};
use thiserror::Error;

use crate::lockfile::LockfileError;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ChecksumError {
    #[error("Could not read index file")]
    CouldNotReadFile(std::io::Error),
    #[error("Could not write to index file")]
    CouldNotWriteFile(std::io::Error),
    #[error("Index contents did not match checksum")]
    BadChecksum,
    #[error("Couldn't get lock on index")]
    NoLock(#[from] LockfileError),
}

const CHECKSUM_SIZE: usize = 20;
pub struct Checksum<'a, T>
where
    T: Read + Write,
{
    file: &'a mut T,
    digest: Sha1,
}

impl<'a, T> Checksum<'a, T>
where
    T: Read + Write,
{
    pub fn new(file: &'a mut T) -> Self {
        let digest = Sha1::new();
        Self { file, digest }
    }

    pub fn read(&mut self, size: usize) -> Result<Vec<u8>, ChecksumError> {
        let mut data = vec![0; size];
        self.file
            .read_exact(&mut data)
            .map_err(ChecksumError::CouldNotReadFile)?;

        self.digest.update(&data);
        Ok(data)
    }

    pub fn verify_checksum(&mut self) -> Result<(), ChecksumError> {
        let mut data = vec![0; CHECKSUM_SIZE];
        self.file
            .read_exact(&mut data)
            .map_err(ChecksumError::CouldNotReadFile)?;

        if self.digest.clone().finalize().as_slice() != data {
            Err(ChecksumError::BadChecksum)
        } else {
            Ok(())
        }
    }

    pub fn write(&mut self, bytes: &[u8]) -> Result<(), ChecksumError> {
        self.file
            .write_all(bytes)
            .map_err(ChecksumError::CouldNotWriteFile)?;
        self.digest.update(bytes);
        Ok(())
    }

    pub fn write_checksum(self) -> Result<(), ChecksumError> {
        let digest = self.digest.finalize();

        self.file
            .write_all(&digest)
            .map_err(ChecksumError::CouldNotWriteFile)?;
        Ok(())
    }
}
