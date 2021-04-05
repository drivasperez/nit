use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs::{File, Metadata},
    io::{Read, Write},
    os::unix::prelude::{MetadataExt, OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};

use thiserror::Error;

use sha1::{Digest, Sha1};

use crate::{
    database::ObjectId,
    lockfile::{Lockfile, LockfileError},
    utils::{drain_to_array, is_executable},
};

const MAX_PATH_SIZE: u16 = 0xfff;
const REGULAR_MODE: u32 = 0o100644;
const EXECUTABLE_MODE: u32 = 0o100755;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum IndexError {
    #[error("Could not write to lockfile: {0}")]
    Lockfile(#[from] LockfileError),
    #[error("Could not access index file: {0}")]
    NoIndexFile(#[from] std::io::Error),
    #[error("Index's digest was uninitialised")]
    DigestError,
    #[error("Error reading checksum: {0}")]
    InvalidChecksum(#[from] ChecksumError),
    #[error("Could not parse index header")]
    BadHeader,
    #[error("Incorrect version, expected {}, got {0}", VERSION)]
    IncorrectVersion(u32),
    #[error("Incorrect signature, expected {}, got {0}", SIGNATURE)]
    IncorrectSignature(String),
}

pub struct Index {
    pathname: PathBuf,
    lockfile: Lockfile,
    entries: BTreeMap<OsString, Entry>,
    changed: bool,
}

const HEADER_SIZE: usize = 12;
const SIGNATURE: &str = "DIRC";
const VERSION: u32 = 2;

impl Index {
    pub fn new(path: impl AsRef<Path>) -> Self {
        let lockfile = Lockfile::new(path.as_ref());
        Self {
            lockfile,
            pathname: path.as_ref().to_owned(),
            entries: BTreeMap::new(),
            changed: false,
        }
    }

    pub fn add(&mut self, path: impl Into<OsString>, oid: ObjectId, metadata: Metadata) {
        let path = path.into();
        let entry = Entry::new(&path, oid, metadata);
        self.entries.insert(path, entry);
        self.changed = true;
    }

    pub fn load_for_update(&mut self) -> Result<(), IndexError> {
        self.load()?;
        Ok(())
    }

    pub fn write_updates(&mut self) -> Result<(), IndexError> {
        if !self.changed {
            self.lockfile.rollback()?;
        }

        dbg!(&self.lockfile);
        let has_lock = self.lockfile.hold_for_update()?;

        if !has_lock {
            panic!("Couldn't write updates")
        };

        let mut writer = Checksum::new(&mut self.lockfile);

        let mut header: Vec<u8> = Vec::new();
        header.extend_from_slice(SIGNATURE.as_bytes());
        header.extend_from_slice(&VERSION.to_be_bytes());
        header.extend_from_slice(&(self.entries.len() as u32).to_be_bytes());

        writer.write(&header)?;

        let mut body = Vec::new();
        for entry in self.entries.values() {
            body.extend_from_slice(&entry.bytes());
        }

        writer.write(&body)?;

        writer.write_checksum()?;

        self.lockfile.commit()?;
        self.changed = false;

        Ok(())
    }

    fn clear(&mut self) {
        self.entries = BTreeMap::new();
        self.changed = false;
    }

    fn load(&mut self) -> Result<(), IndexError> {
        self.clear();
        let file = self.open_index_file()?;

        if let Some(mut f) = file {
            let mut reader = Checksum::new(&mut f);
            let count = self.read_header(&mut reader)?;
            self.read_entries(&mut reader, count)?;
            reader.verify_checksum()?;
        }

        Ok(())
    }

    fn open_index_file(&self) -> Result<Option<File>, IndexError> {
        match File::open(&self.pathname) {
            Ok(f) => Ok(Some(f)),
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    Ok(None)
                } else {
                    Err(e.into())
                }
            }
        }
    }

    fn read_header<T: Read + Write>(&self, reader: &mut Checksum<T>) -> Result<usize, IndexError> {
        let data = reader.read(HEADER_SIZE)?;
        let signature = std::str::from_utf8(&data[0..4]).map_err(|_| IndexError::BadHeader)?;

        let mut version = [0; 4];
        version.clone_from_slice(&data[4..8]);
        let version = u32::from_be_bytes(version);

        let mut count = [0; 4];
        count.clone_from_slice(&data[8..12]);
        let count = u32::from_be_bytes(count);

        if signature != SIGNATURE {
            return Err(IndexError::IncorrectSignature(signature.to_owned()));
        }

        if version != VERSION {
            return Err(IndexError::IncorrectVersion(version));
        }

        Ok(count as usize)
    }

    fn read_entries<T: Read + Write>(
        &mut self,
        reader: &mut Checksum<T>,
        count: usize,
    ) -> Result<(), IndexError> {
        // Entries are at least 64 bytes...
        const ENTRY_MIN_SIZE: usize = 64;
        // ...and are padded with null bytes to always have a length divisible by 8.
        const ENTRY_BLOCK: usize = 8;

        for _ in 0..count {
            let mut entry = reader.read(ENTRY_MIN_SIZE)?;

            // Entries are null-terminated.
            // We just read 64 bytes into this vector so we can unwrap .last().
            while entry.last().unwrap() != &b'\0' {
                entry.extend_from_slice(&reader.read(ENTRY_BLOCK)?);
            }

            self.store_entry(Entry::parse(entry)?);
        }

        Ok(())
    }

    fn store_entry(&mut self, entry: Entry) {
        self.entries.insert(entry.path.clone(), entry);
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ChecksumError {
    #[error("Could not read index file: {0}")]
    CouldNotReadFile(#[from] std::io::Error),
    #[error("Index contents did not match checksum")]
    BadChecksum,
    #[error("Couldn't get lock on index: {0}")]
    NoLock(#[from] LockfileError),
}

const CHECKSUM_SIZE: usize = 20;
struct Checksum<'a, T>
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
        self.file.read_exact(&mut data)?;

        self.digest.update(&data);
        Ok(data)
    }

    pub fn verify_checksum(&mut self) -> Result<(), ChecksumError> {
        let mut data = vec![0; CHECKSUM_SIZE];
        self.file.read_exact(&mut data)?;

        if self.digest.clone().finalize().as_slice() != data {
            Err(ChecksumError::BadChecksum)
        } else {
            Ok(())
        }
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), IndexError> {
        self.file.write_all(bytes)?;
        self.digest.update(bytes);
        Ok(())
    }

    fn write_checksum(self) -> Result<(), IndexError> {
        let digest = self.digest.finalize();

        self.file.write_all(&digest)?;
        Ok(())
    }
}

pub struct Entry {
    ctime: u32,
    ctime_nsec: u32,
    mtime: u32,
    mtime_nsec: u32,
    dev: u32,
    ino: u32,
    mode: u32,
    uid: u32,
    gid: u32,
    size: u32,
    oid: ObjectId,
    flags: u16,
    path: OsString,
}

impl Entry {
    pub fn new(path: &OsStr, oid: ObjectId, stat: Metadata) -> Self {
        let ctime = stat.ctime() as u32;
        let ctime_nsec = stat.ctime_nsec() as u32;
        let mtime = stat.mtime() as u32;
        let mtime_nsec = stat.mtime_nsec() as u32;
        let dev = stat.dev() as u32;
        let ino = stat.ino() as u32;
        let uid = stat.uid() as u32;
        let gid = stat.gid() as u32;
        let size = stat.size() as u32;
        let mode = if is_executable(stat.mode()) {
            EXECUTABLE_MODE
        } else {
            REGULAR_MODE
        };

        let flags = u16::min(path.as_bytes().len() as u16, MAX_PATH_SIZE);

        let path = path.to_owned();

        Self {
            ctime,
            ctime_nsec,
            mtime,
            mtime_nsec,
            dev,
            ino,
            mode,
            uid,
            gid,
            size,
            oid,
            flags,
            path,
        }
    }

    pub fn bytes(&self) -> Vec<u8> {
        const ENTRY_BLOCK: usize = 8;

        let mut bytes = Vec::new();

        let Self {
            ctime,
            ctime_nsec,
            mtime,
            mtime_nsec,
            dev,
            ino,
            mode,
            uid,
            gid,
            size,
            oid,
            flags,
            path,
        } = &self;

        for &item in &[
            ctime, ctime_nsec, mtime, mtime_nsec, dev, ino, mode, uid, gid, size,
        ] {
            let bs = item.to_be_bytes();
            bytes.extend_from_slice(&bs);
        }

        bytes.extend_from_slice(oid.bytes());
        bytes.extend_from_slice(&flags.to_be_bytes());
        bytes.extend_from_slice(path.as_bytes());
        bytes.extend_from_slice(b"\0");

        while bytes.len() % ENTRY_BLOCK != 0 {
            bytes.push(b'\0');
        }

        bytes
    }

    pub fn parse(mut data: Vec<u8>) -> Result<Self, IndexError> {
        let ctime = 0_u32;
        let ctime_nsec = 0_u32;
        let mtime = 0_u32;
        let mtime_nsec = 0_u32;
        let dev = 0_u32;
        let ino = 0_u32;
        let mode = 0_u32;
        let uid = 0_u32;
        let gid = 0_u32;
        let size = 0_u32;

        for item in &mut [
            ctime, ctime_nsec, mtime, mtime_nsec, dev, ino, mode, uid, gid, size,
        ] {
            let arr: [u8; 4] = drain_to_array(&mut data);
            *item = u32::from_be_bytes(arr);
        }

        let oid = drain_to_array(&mut data).into();

        let arr = drain_to_array(&mut data);
        let flags = u16::from_be_bytes(arr);

        let path: Vec<_> = data.into_iter().take_while(|&b| b != b'\0').collect();
        let path = OsString::from_vec(path);

        Ok(Self {
            ctime,
            ctime_nsec,
            mtime,
            mtime_nsec,
            dev,
            ino,
            mode,
            uid,
            gid,
            size,
            oid,
            flags,
            path,
        })
    }
}
