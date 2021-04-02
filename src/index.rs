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
const REGULAR_MODE: u32 = 0o100644;
const EXECUTABLE_MODE: u32 = 0o100755;

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
