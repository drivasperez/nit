use anyhow::Context;
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use std::{fmt::Display, path::PathBuf};
use std::{io, path::Path};

use std::fs;
use std::fs::File;
use std::io::prelude::*;

use flate2::{write::DeflateEncoder, Compression};
use sha1::Sha1;

pub struct Workspace {
    pathname: PathBuf,
}

impl Workspace {
    pub fn new<P: Into<PathBuf>>(pathname: P) -> Self {
        Self {
            pathname: pathname.into(),
        }
    }

    pub fn list_files(&self) -> std::io::Result<Vec<PathBuf>> {
        let dirs = std::fs::read_dir(&self.pathname)?;
        let mut filtered_dirs = Vec::new();
        for dir in dirs {
            let path = dir?.path();
            if !&[".", "..", ".git"].iter().any(|&s| path.ends_with(s)) {
                filtered_dirs.push(path);
            }
        }

        Ok(filtered_dirs)
    }

    pub fn read_file<P: AsRef<Path>>(&self, path: P) -> std::io::Result<Vec<u8>> {
        std::fs::read(&self.pathname.join(&path))
    }
}

pub struct Database {
    pathname: PathBuf,
}

impl Database {
    pub fn new<P: Into<PathBuf>>(pathname: P) -> Self {
        Self {
            pathname: pathname.into(),
        }
    }

    pub fn store(&self, object: &mut Object) -> anyhow::Result<()> {
        let mut content = Vec::new();
        let data = object.to_bytestr();
        content.extend_from_slice(object.kind().as_string().as_bytes());
        content.extend_from_slice(b" ");
        content.extend_from_slice(&data.len().to_string().as_bytes());
        content.extend_from_slice(b" \0");
        content.extend_from_slice(data);

        let hash = Sha1::from(&content);
        let oid = hash.digest().bytes();
        self.write_object(&oid, &content)
            .with_context(|| format!("Couldn't write object with hash {:?}", &oid))?;
        object.set_oid(oid);

        Ok(())
    }

    fn write_object(&self, oid: &[u8; 20], content: &[u8]) -> anyhow::Result<()> {
        let hash = bytes_to_hex_string(oid)?;
        let dir = &hash[0..2];
        let obj = &hash[3..];

        let object_path = self.pathname.join(dir).join(obj);

        let dirname = object_path
            .parent()
            .with_context(|| format!("Couldn't get directory from {:?}", object_path))?;

        let temp_path = dirname.join(Database::generate_temp_name());

        let file = File::create(&temp_path)
            .or_else(|e| match e.kind() {
                io::ErrorKind::NotFound => {
                    fs::create_dir_all(dirname).and_then(|_| File::create(&temp_path))
                }
                _ => Err(e),
            })
            .context("Couldn't create file to write to")?;
        let mut encoder = DeflateEncoder::new(file, Compression::default());

        encoder
            .write_all(content)
            .context("Couldn't hash contents of blob")?;
        encoder.finish()?;

        std::fs::rename(temp_path, object_path)?;

        Ok(())
    }

    fn generate_temp_name() -> String {
        let blah: Vec<u8> = thread_rng().sample_iter(&Alphanumeric).take(6).collect();
        String::from_utf8(blah).unwrap()
    }
}

#[derive(Copy, Clone, Debug)]
pub enum ObjectKind {
    Blob,
    Tree,
}

impl ObjectKind {
    pub fn as_string(&self) -> String {
        format!("{}", self)
    }
}

impl Display for ObjectKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Blob => write!(f, "blob"),
            Self::Tree => write!(f, "tree"),
        }
    }
}

pub struct Object {
    oid: Option<[u8; 20]>,
    data: Vec<u8>,
    kind: ObjectKind,
}

impl Object {
    pub fn new(kind: ObjectKind, data: Vec<u8>) -> Self {
        Self {
            data,
            oid: None,
            kind,
        }
    }

    fn kind(&self) -> ObjectKind {
        ObjectKind::Blob
    }

    fn to_bytestr(&self) -> &[u8] {
        &self.data
    }

    fn set_oid(&mut self, oid: [u8; 20]) {
        self.oid = Some(oid);
    }

    fn oid(&self) -> Option<&[u8; 20]> {
        self.oid.as_ref()
    }
}

fn bytes_to_hex_string(bytes: &[u8]) -> anyhow::Result<String> {
    use core::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(s, "{:02x}", byte)?;
    }

    Ok(s)
}
