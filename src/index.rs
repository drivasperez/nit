use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    fs::Metadata,
    os::unix::prelude::{MetadataExt, OsStrExt},
    path::Path,
};

use anyhow::anyhow;

use sha1::{Digest, Sha1};

use crate::{database::ObjectId, lockfile::Lockfile, utils::is_executable};

pub struct Index {
    lockfile: Lockfile,
    entries: HashMap<OsString, Entry>,
    digest: Option<Sha1>,
}

const MAX_PATH_SIZE: u16 = 0xfff;
const REGULAR_MODE: u16 = 0o100644;
const EXECUTABLE_MODE: u16 = 0o100755;

pub struct Entry {
    ctime: i64,
    ctime_nsec: i64,
    mtime: i64,
    mtime_nsec: i64,
    dev: u64,
    ino: u64,
    mode: u16,
    uid: u32,
    gid: u32,
    size: u64,
    oid: ObjectId,
    flags: usize,
    path: OsString,
}

impl Entry {
    pub fn new(path: &OsStr, oid: ObjectId, stat: Metadata) -> Self {
        let ctime = stat.ctime();
        let ctime_nsec = stat.ctime_nsec();
        let mtime = stat.mtime();
        let mtime_nsec = stat.mtime_nsec();
        let dev = stat.dev();
        let ino = stat.ino();
        let uid = stat.uid();
        let gid = stat.gid();
        let size = stat.size();
        let mode = if is_executable(stat.mode()) {
            EXECUTABLE_MODE
        } else {
            REGULAR_MODE
        };

        let flags = usize::min(path.as_bytes().len(), MAX_PATH_SIZE as usize);

        let path = path.to_owned();

        Self {
            ctime,
            ctime_nsec,
            mtime,
            mtime_nsec,
            dev,
            mode,
            path,
            uid,
            gid,
            oid,
            flags,
            size,
            ino,
        }
    }

    pub fn bytes(&self) -> Vec<u8> {
        let bytes = Vec::new();
        bytes
    }
}

impl Index {
    pub fn new(path: impl AsRef<Path>) -> Self {
        let lockfile = Lockfile::new(path.as_ref());
        Self {
            lockfile,
            entries: HashMap::new(),
            digest: None,
        }
    }

    pub fn add(&mut self, path: impl Into<OsString>, oid: ObjectId, metadata: Metadata) {
        let path = path.into();
        let entry = Entry::new(&path, oid, metadata);
        self.entries.insert(path, entry);
    }

    pub fn write_updates(&mut self) -> anyhow::Result<()> {
        self.lockfile.hold_for_update()?;

        self.begin_write();
        let mut header: Vec<u8> = Vec::new();
        header.extend_from_slice(b"DIRC");
        header.extend_from_slice(&2_u32.to_be_bytes());
        header.extend_from_slice(&(self.entries.len() as u32).to_be_bytes());
        self.write(&header)?;
        let mut body = Vec::new();
        for (_, entry) in &self.entries {
            body.extend_from_slice(&entry.bytes());
        }
        self.write(&body)?;

        self.finish_write()?;

        Ok(())
    }

    fn begin_write(&mut self) {
        self.digest = Some(Sha1::new());
    }

    fn write(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        self.lockfile.write(&bytes)?;
        self.digest
            .as_mut()
            .ok_or_else(|| anyhow!("Index digest was uninitialised"))?
            .update(bytes);
        Ok(())
    }

    fn finish_write(&mut self) -> anyhow::Result<()> {
        let digest = self
            .digest
            .take()
            .ok_or_else(|| anyhow!("Index digest was uninitialised"))?
            .finalize();

        self.lockfile.write(&digest)?;
        self.lockfile.commit()?;
        Ok(())
    }
}
