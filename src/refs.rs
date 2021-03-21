use crate::database::ObjectId;
use crate::lockfile::Lockfile;
use std::path::{Path, PathBuf};

pub struct Refs {
    pathname: PathBuf,
}

impl Refs {
    pub fn new(pathname: &Path) -> Self {
        Self {
            pathname: pathname.to_owned(),
        }
    }
    pub fn head_path(&self) -> PathBuf {
        self.pathname.join("HEAD")
    }

    pub fn update_head(&self, oid: &ObjectId) -> anyhow::Result<()> {
        let mut lock = Lockfile::new(&self.head_path());
        lock.hold_for_update()?;

        lock.write(&oid.as_str()?)?;
        lock.write("\n")?;

        lock.commit()?;

        Ok(())
    }

    pub fn read_head(&self) -> Option<String> {
        let bytes = std::fs::read(self.head_path()).ok()?;
        let s = String::from_utf8(bytes).ok()?;

        Some(s)
    }
}
