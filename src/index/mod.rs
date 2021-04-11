use crate::{database::ObjectId, lockfile::Lockfile, utils::drain_to_array};

use crate::Result;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    ffi::{OsStr, OsString},
    fs::{File, Metadata},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use thiserror::Error;

pub mod checksum;
pub mod entry;

use checksum::Checksum;
use entry::Entry;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum IndexError {
    #[error("Could not access index file")]
    NoIndexFile(#[from] std::io::Error),
    #[error("Index's digest was uninitialised")]
    DigestError,
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

    pub fn load(&mut self) -> Result<()> {
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

    pub fn load_for_update(&mut self) -> Result<()> {
        self.load()
    }

    pub fn write_updates(&mut self) -> Result<()> {
        if !self.changed {
            self.lockfile.rollback()?;
        }

        self.lockfile.hold_for_update()?;

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

    fn open_index_file(&self) -> Result<Option<File>> {
        let res: Result<_, IndexError> = match File::open(&self.pathname) {
            Ok(f) => Ok(Some(f)),
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    Ok(None)
                } else {
                    Err(e.into())
                }
            }
        };

        Ok(res?)
    }

    fn read_header<T: Read + Write>(&self, reader: &mut Checksum<T>) -> Result<usize> {
        let mut data = reader.read(HEADER_SIZE)?;
        let signature: [u8; 4] = drain_to_array(&mut data);
        let signature = std::str::from_utf8(&signature).map_err(|_| IndexError::BadHeader)?;

        let version = u32::from_be_bytes(drain_to_array(&mut data));

        let count = u32::from_be_bytes(drain_to_array(&mut data));

        if signature != SIGNATURE {
            return Err(IndexError::IncorrectSignature(signature.to_owned()).into());
        }

        if version != VERSION {
            return Err(IndexError::IncorrectVersion(version).into());
        }

        Ok(count as usize)
    }

    fn read_entries<T: Read + Write>(
        &mut self,
        reader: &mut Checksum<T>,
        count: usize,
    ) -> Result<()> {
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

    /// Get a mutable reference to the index's lockfile.
    pub fn lockfile_mut(&mut self) -> &mut Lockfile {
        &mut self.lockfile
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
