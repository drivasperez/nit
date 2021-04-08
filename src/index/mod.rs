use crate::{
    database::ObjectId,
    lockfile::{Lockfile, LockfileError},
};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    ffi::{OsStr, OsString},
    fs::{File, Metadata},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use thiserror::Error;

mod checksum;
pub mod entry;

use checksum::{Checksum, ChecksumError};
use entry::Entry;

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
    parents: HashMap<OsString, HashSet<OsString>>,
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
            parents: HashMap::new(),
            changed: false,
        }
    }

    pub fn add(&mut self, path: impl Into<OsString>, oid: ObjectId, metadata: Metadata) {
        let path = path.into();
        let entry = Entry::new(&path, oid, metadata);
        self.discard_conflicts(&entry);
        self.store_entry(entry);
        self.changed = true;
    }

    pub fn entries(&self) -> &BTreeMap<OsString, Entry> {
        &self.entries
    }

    pub fn load(&mut self) -> Result<(), IndexError> {
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

    pub fn load_for_update(&mut self) -> Result<(), IndexError> {
        self.load()
    }

    pub fn write_updates(&mut self) -> Result<(), IndexError> {
        if !self.changed {
            self.lockfile.rollback()?;
        }

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
        self.entries.clear();
        self.parents.clear();
        self.changed = false;
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
            // We just read 64 bytes into this vector so we can safely unwrap .last().
            while entry.last().unwrap() != &b'\0' {
                entry.extend_from_slice(&reader.read(ENTRY_BLOCK)?);
            }

            let entry = Entry::parse(entry)?;
            self.store_entry(entry);
        }

        Ok(())
    }

    fn store_entry(&mut self, entry: Entry) {
        for dirname in &entry.parent_directories() {
            self.parents
                .entry(dirname.into())
                .or_insert_with(HashSet::new)
                .insert(entry.path().to_owned());
        }
        self.entries.insert(entry.path().clone(), entry);
    }

    fn discard_conflicts(&mut self, entry: &Entry) {
        for path in entry.parent_directories() {
            self.entries.remove(path.as_os_str());
        }

        self.remove_children(entry.path());
    }

    fn remove_children(&mut self, path: &OsStr) {
        if let Some(children) = self.parents.get(path) {
            for child in children.clone() {
                self.remove_entry(&child);
            }
        }
    }

    fn remove_entry(&mut self, path: &OsStr) -> Option<Entry> {
        let entry = self.entries.get(path)?;

        for dirname in &entry.parent_directories() {
            let map = self.parents.get_mut(dirname.as_os_str())?;
            map.remove(entry.path());
            if map.is_empty() {
                self.parents.remove(dirname.as_os_str());
            }
        }

        self.entries.remove(path)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::path::PathBuf;

    struct Scaffold {
        index: Index,
        oid: ObjectId,
        stat: Metadata,
    }

    fn startup() -> Scaffold {
        let f = PathBuf::from(file!());
        let current_dir = f.parent().unwrap();
        let tmp_path = std::fs::canonicalize(PathBuf::from(current_dir).join("../../tmp")).unwrap();
        let index_path = tmp_path.join("index");

        let stat = std::fs::metadata(file!()).unwrap();
        let oid = ObjectId::from([12; 20]);

        Scaffold {
            index: Index::new(index_path),
            stat,
            oid,
        }
    }

    #[test]
    fn adds_a_single_file() {
        let Scaffold {
            mut index,
            stat,
            oid,
        } = startup();

        index.add("alice.txt", oid, stat);
        assert_eq!(
            vec!["alice.txt"],
            index.entries().keys().cloned().collect::<Vec<OsString>>()
        );
    }

    #[test]
    fn replaces_a_file_with_a_directory() {
        let Scaffold {
            mut index,
            stat,
            oid,
        } = startup();

        index.add("alice.txt", oid.clone(), stat.clone());
        index.add("bob.txt", oid.clone(), stat.clone());

        index.add("alice.txt/nested.txt", oid, stat);

        assert_eq!(
            vec!["alice.txt/nested.txt", "bob.txt"],
            index.entries().keys().cloned().collect::<Vec<OsString>>()
        );
    }

    #[test]
    fn replaces_a_directory_with_a_file() {
        let Scaffold {
            mut index,
            stat,
            oid,
        } = startup();

        index.add("alice.txt", oid.clone(), stat.clone());
        index.add("nested/bob.txt", oid.clone(), stat.clone());

        index.add("nested", oid, stat);

        assert_eq!(
            vec!["alice.txt", "nested"],
            index.entries().keys().cloned().collect::<Vec<OsString>>()
        );
    }

    #[test]
    fn recursively_replaces_a_directory_with_a_file() {
        let Scaffold {
            mut index,
            stat,
            oid,
        } = startup();

        index.add("alice.txt", oid.clone(), stat.clone());
        index.add("nested/bob.txt", oid.clone(), stat.clone());
        index.add("nested/inner/claire.txt", oid.clone(), stat.clone());
        index.add("nested/another_inner/eve.txt", oid.clone(), stat.clone());

        index.add("nested", oid, stat);

        assert_eq!(
            vec!["alice.txt", "nested"],
            index.entries().keys().cloned().collect::<Vec<OsString>>()
        );
    }
}
