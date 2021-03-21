use std::{
    borrow::Cow,
    fmt::Display,
    fs::{self, File},
    io::{self, Write},
    path::PathBuf,
};

use anyhow::Context;
use flate2::{write::DeflateEncoder, Compression};
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use sha1::{Digest, Sha1};

#[derive(Debug, Clone)]
pub struct Oid([u8; 20]);

impl Oid {
    pub fn as_str(&self) -> anyhow::Result<String> {
        bytes_to_hex_string(&self.0)
    }

    pub fn bytes(&self) -> &[u8; 20] {
        &self.0
    }
}

impl Display for Oid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = self
            .as_str()
            .unwrap_or_else(|_| String::from("[Invalid Oid]"));
        write!(f, "{}", s)
    }
}

pub trait Object {
    fn data(&self) -> Cow<[u8]>;
    fn kind(&self) -> &str;
    fn set_oid(&mut self, oid: Oid);
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

    pub fn store<O: Object>(&self, object: &mut O) -> anyhow::Result<()> {
        let mut content = Vec::new();
        let data = object.data();
        content.extend_from_slice(object.kind().as_bytes());
        content.extend_from_slice(b" ");
        content.extend_from_slice(&data.len().to_string().as_bytes());
        content.extend_from_slice(b"\0");
        content.extend_from_slice(&data);

        let hash = Sha1::digest(&content);
        let oid = Oid(hash.into());
        self.write_object(&oid, &content)
            .with_context(|| format!("Couldn't write object with hash {:?}", &oid))?;
        object.set_oid(oid);

        Ok(())
    }

    fn write_object(&self, oid: &Oid, content: &[u8]) -> anyhow::Result<()> {
        let hash = oid.as_str()?;
        let dir = &hash[0..2];
        let obj = &hash[2..];

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
        let mut encoder = DeflateEncoder::new(file, Compression::fast());

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

fn bytes_to_hex_string(bytes: &[u8]) -> anyhow::Result<String> {
    use core::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(s, "{:02x}", byte)?;
    }

    Ok(s)
}
