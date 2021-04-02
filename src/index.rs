use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    fs::Metadata,
    os::unix::prelude::{MetadataExt, OsStrExt},
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::{
    database::{EntryMode, ObjectId},
    lockfile::Lockfile,
    utils::is_executable,
};

pub struct Index {
    lockfile: Lockfile,
    entries: HashMap<OsString, Entry>,
}

const MAX_PATH_SIZE: usize = 0xfff;
const REGULAR_MODE: &str = "0100644";
const EXECUTABLE_MODE: &str = "0100755";

pub struct Entry {
    ctime: i64,
    ctime_nsec: i64,
    mtime: i64,
    mtime_nsec: i64,
    dev: u64,
    ino: u64,
    mode: &'static str,
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

        let flags = usize::min(path.as_bytes().len(), MAX_PATH_SIZE);

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
}

impl Index {
    pub fn new(path: impl AsRef<Path>) -> Self {
        let lockfile = Lockfile::new(path.as_ref());
        Self {
            lockfile,
            entries: HashMap::new(),
        }
    }

    pub fn add(&mut self, path: impl Into<OsString>, oid: ObjectId, metadata: Metadata) {
        let path = path.into();
        let entry = Entry::new(&path, oid, metadata);
        self.entries.insert(path, entry);
    }

    pub fn write_updates(&self) -> anyhow::Result<()> {
        todo!()
    }
}
