use std::{
    borrow::Cow,
    fmt::{Debug, Display},
    fs::{self, File},
    io::{self, Write},
    path::PathBuf,
};

use crate::utils::bytes_to_hex_string;

use flate2::{write::ZlibEncoder, Compression};
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use sha1::{Digest, Sha1};
use thiserror::Error;

mod author;
mod blob;
mod commit;
mod tree;

pub use author::*;
pub use blob::*;
pub use commit::*;
pub use tree::*;

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("Couldn't read oid")]
    BadObjectId(#[from] std::fmt::Error),
    #[error("Couldn't get object's parent directory: {0}")]
    NoParent(PathBuf),
    #[error("IO rror while writing: {0}")]
    CouldNotWrite(#[from] std::io::Error),
}
#[derive(PartialEq, Clone)]
pub struct ObjectId([u8; 20]);

impl ObjectId {
    pub fn as_str(&self) -> Result<String, std::fmt::Error> {
        bytes_to_hex_string(&self.0)
    }

    pub fn bytes(&self) -> &[u8; 20] {
        &self.0
    }
}

impl Debug for ObjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = self
            .as_str()
            .unwrap_or_else(|_| String::from("[Invalid Oid]"));
        write!(f, "{}", s)
    }
}

impl Display for ObjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = self.as_str()?;
        write!(f, "{}", s)
    }
}

impl From<[u8; 20]> for ObjectId {
    fn from(arr: [u8; 20]) -> Self {
        Self(arr)
    }
}

pub trait Object {
    fn data(&self) -> Cow<[u8]>;
    fn kind(&self) -> &str;
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

    pub fn store<O: Object>(&self, object: &O) -> Result<ObjectId, DatabaseError> {
        let mut content = Vec::new();
        let data = object.data();
        content.extend_from_slice(object.kind().as_bytes());
        content.extend_from_slice(b" ");
        content.extend_from_slice(&data.len().to_string().as_bytes());
        content.extend_from_slice(b"\0");
        content.extend_from_slice(&data);

        let hash = Sha1::digest(&content);
        let oid = ObjectId(hash.into());
        self.write_object(&oid, &content)?;

        Ok(oid)
    }

    fn write_object(&self, oid: &ObjectId, content: &[u8]) -> Result<(), DatabaseError> {
        let hash = oid.as_str()?;
        let dir = &hash[0..2];
        let obj = &hash[2..];

        let object_path = self.pathname.join(dir).join(obj);

        if object_path.exists() {
            return Ok(());
        }

        let dirname = object_path
            .parent()
            .ok_or_else(|| DatabaseError::NoParent(object_path.clone()))?;

        let temp_path = dirname.join(Database::generate_temp_name());

        let file = File::create(&temp_path).or_else(|e| match e.kind() {
            io::ErrorKind::NotFound => {
                fs::create_dir_all(dirname).and_then(|_| File::create(&temp_path))
            }
            _ => Err(e),
        })?;
        let mut encoder = ZlibEncoder::new(file, Compression::fast());

        encoder.write_all(content)?;
        encoder.finish()?;

        std::fs::rename(temp_path, object_path)?;

        Ok(())
    }

    // TODO: Not thread-safe.
    fn generate_temp_name() -> String {
        let blah: Vec<u8> = thread_rng().sample_iter(&Alphanumeric).take(6).collect();
        String::from_utf8(blah).unwrap()
    }
}
