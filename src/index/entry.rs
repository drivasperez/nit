use crate::utils::{drain_to_array, is_executable};
use std::{
    ffi::OsString,
    fs::Metadata,
    os::unix::prelude::{MetadataExt, OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};

use crate::database::ObjectId;
use crate::Result;

const MAX_PATH_SIZE: u16 = 0xfff;
const REGULAR_MODE: u32 = 0o100644;
const EXECUTABLE_MODE: u32 = 0o100755;

#[derive(Debug, Clone, PartialEq)]
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
    path: PathBuf,
}

impl Entry {
    pub fn new(path: &impl AsRef<Path>, oid: ObjectId, stat: Metadata) -> Self {
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

        let path = path.as_ref().to_owned();

        let flags = u16::min(path.as_os_str().as_bytes().len() as u16, MAX_PATH_SIZE);

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

    pub fn parent_directories(&self) -> Vec<PathBuf> {
        let path = PathBuf::from(&self.path);
        let mut directories: Vec<_> = path.ancestors().map(|c| c.to_owned()).skip(1).collect();

        directories.pop();

        directories.into_iter().rev().collect()
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
        bytes.extend_from_slice(path.as_os_str().as_bytes());
        bytes.extend_from_slice(b"\0");

        while bytes.len() % ENTRY_BLOCK != 0 {
            bytes.push(b'\0');
        }

        bytes
    }

    pub fn parse(mut data: Vec<u8>) -> Result<Self> {
        let ctime = u32::from_be_bytes(drain_to_array(&mut data));
        let ctime_nsec = u32::from_be_bytes(drain_to_array(&mut data));
        let mtime = u32::from_be_bytes(drain_to_array(&mut data));
        let mtime_nsec = u32::from_be_bytes(drain_to_array(&mut data));
        let dev = u32::from_be_bytes(drain_to_array(&mut data));
        let ino = u32::from_be_bytes(drain_to_array(&mut data));
        let mode = u32::from_be_bytes(drain_to_array(&mut data));
        let uid = u32::from_be_bytes(drain_to_array(&mut data));
        let gid = u32::from_be_bytes(drain_to_array(&mut data));
        let size = u32::from_be_bytes(drain_to_array(&mut data));

        let oid = drain_to_array(&mut data).into();

        let arr = drain_to_array(&mut data);
        let flags = u16::from_be_bytes(arr);

        let path: Vec<_> = data.into_iter().take_while(|&b| b != b'\0').collect();
        let path = PathBuf::from(OsString::from_vec(path));

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

    /// Get a reference to the entry's path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Get a reference to the entry's mode.
    pub fn mode(&self) -> u32 {
        self.mode
    }

    /// Get a reference to the entry's ObjectId.
    pub fn oid(&self) -> &ObjectId {
        &self.oid
    }
}
